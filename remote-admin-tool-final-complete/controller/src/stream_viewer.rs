use anyhow::Result;
use base64::{Engine as _, engine::general_purpose};
use futures_util::StreamExt;
use serde_json::Value;
use tokio_tungstenite::connect_async;
use minifb::{Window, WindowOptions, Key, Scale};

// Webcam/screen stream viewer (for commands 12 and 16)
pub async fn start_stream_viewer(ws_url: &str) -> Result<()> {
    println!("Connecting to stream: {}", ws_url);
    
    let (ws_stream, _) = connect_async(ws_url).await?;
    println!("✓ Connected! Starting viewer...");
    
    let (_write, mut read) = ws_stream.split();
    
    let mut window: Option<Window> = None;
    let mut frame_count = 0u32;
    let mut last_dimensions = (0, 0);
    
    while let Some(msg) = read.next().await {
        match msg {
            Ok(tokio_tungstenite::tungstenite::Message::Text(text)) => {
                if let Ok(data) = serde_json::from_str::<Value>(&text) {
                    if data["status"].is_string() {
                        println!("Status: {}", data["status"]);
                        continue;
                    }
                    
                    if let Some(frame_data) = data["data"].as_str() {
                        if let Ok(decoded) = general_purpose::STANDARD.decode(frame_data) {
                            frame_count += 1;
                            
                            if let Ok(img) = image::load_from_memory(&decoded) {
                                let rgb_img = img.to_rgb8();
                                let (width, height) = rgb_img.dimensions();
                                
                                if window.is_none() {
                                    let mut options = WindowOptions::default();
                                    options.scale = Scale::X1;
                                    options.resize = true;
                                    
                                    let mut new_window = Window::new(
                                        "Live Stream - Press ESC to exit",
                                        width as usize,
                                        height as usize,
                                        options,
                                    )?;
                                    
                                    new_window.set_target_fps(30);
                                    window = Some(new_window);
                                    last_dimensions = (width, height);
                                    println!("✓ Window opened ({}x{})", width, height);
                                }
                                
                                if last_dimensions != (width, height) && window.is_some() {
                                    drop(window.take());
                                    let mut options = WindowOptions::default();
                                    options.scale = Scale::X1;
                                    options.resize = true;
                                    
                                    let mut new_window = Window::new(
                                        "Live Stream - Press ESC to exit",
                                        width as usize,
                                        height as usize,
                                        options,
                                    )?;
                                    
                                    new_window.set_target_fps(30);
                                    window = Some(new_window);
                                    last_dimensions = (width, height);
                                }
                                
                                let buffer: Vec<u32> = rgb_img
                                    .pixels()
                                    .map(|p| {
                                        let r = p[0] as u32;
                                        let g = p[1] as u32;
                                        let b = p[2] as u32;
                                        (r << 16) | (g << 8) | b
                                    })
                                    .collect();
                                
                                if let Some(ref mut win) = window {
                                    if !win.is_open() || win.is_key_down(Key::Escape) {
                                        println!("\n✓ Stream ended by user");
                                        break;
                                    }
                                    
                                    if let Err(e) = win.update_with_buffer(&buffer, width as usize, height as usize) {
                                        eprintln!("Window error: {}", e);
                                        break;
                                    }
                                    
                                    if frame_count % 30 == 0 {
                                        println!("📺 Frame #{}", frame_count);
                                    }
                                }
                            }
                        }
                    }
                }
            }
            Ok(tokio_tungstenite::tungstenite::Message::Close(_)) => {
                println!("\n✓ Stream closed");
                break;
            }
            Err(e) => {
                eprintln!("WebSocket error: {}", e);
                break;
            }
            _ => {}
        }
    }
    
    println!("\n✓ Stream ended. Total frames: {}", frame_count);
    drop(window);
    Ok(())
}

// Remote desktop viewer with control (for command 18)
pub async fn start_remote_desktop(
    ws_url: &str, 
    _agent_ip: &str,
    controller: &mut crate::Controller
) -> Result<()> {
    println!("Connecting to remote desktop: {}", ws_url);
    
    let (ws_stream, _) = connect_async(ws_url).await?;
    println!("✓ Connected! Controlling remote desktop...");
    
    let (_write, mut read) = ws_stream.split();
    
    let mut window: Option<Window> = None;
    let mut frame_count = 0u32;
    let mut last_dimensions = (0, 0);
    let mut mouse_buttons_pressed = std::collections::HashSet::new();
    
    println!("\n🖥️ Remote Desktop Control Active!");
    println!("   Click and type in the window to control the remote system");
    println!("   Press ESC to exit\n");
    
    while let Some(msg) = read.next().await {
        match msg {
            Ok(tokio_tungstenite::tungstenite::Message::Text(text)) => {
                if let Ok(data) = serde_json::from_str::<serde_json::Value>(&text) {
                    if data["status"].is_string() {
                        println!("Status: {}", data["status"]);
                        continue;
                    }
                    
                    if let Some(frame_data) = data["data"].as_str() {
                        if let Ok(decoded) = general_purpose::STANDARD.decode(frame_data) {
                            frame_count += 1;
                            
                            if let Ok(img) = image::load_from_memory(&decoded) {
                                let rgb_img = img.to_rgb8();
                                let (width, height) = rgb_img.dimensions();
                                
                                // Create or recreate window
                                if window.is_none() || last_dimensions != (width, height) {
                                    drop(window.take());
                                    
                                    let mut options = WindowOptions::default();
                                    options.scale = Scale::X1;
                                    options.resize = true;
                                    
                                    let mut new_window = Window::new(
                                        "Remote Desktop - Click to control - Press ESC to exit",
                                        width as usize,
                                        height as usize,
                                        options,
                                    )?;
                                    
                                    new_window.set_target_fps(30);
                                    window = Some(new_window);
                                    last_dimensions = (width, height);
                                    println!("✓ Remote desktop window opened ({}x{})", width, height);
                                }
                                
                                let buffer: Vec<u32> = rgb_img
                                    .pixels()
                                    .map(|p| {
                                        let r = p[0] as u32;
                                        let g = p[1] as u32;
                                        let b = p[2] as u32;
                                        (r << 16) | (g << 8) | b
                                    })
                                    .collect();
                                
                                if let Some(ref mut win) = window {
                                    if !win.is_open() || win.is_key_down(Key::Escape) {
                                        println!("\n✓ Remote desktop session ended");
                                        break;
                                    }
                                    
                                    // HANDLE MOUSE CLICKS - SEND COMMANDS TO AGENT
                                    if let Some((mx, my)) = win.get_mouse_pos(minifb::MouseMode::Clamp) {
                                        let x = mx as i32;
                                        let y = my as i32;
                                        
                                        // Left click
                                        if win.get_mouse_down(minifb::MouseButton::Left) {
                                            if !mouse_buttons_pressed.contains(&"left") {
                                                println!("🖱️ Left click at ({}, {})", x, y);
                                                mouse_buttons_pressed.insert("left");
                                                
                                                // Send MoveMouse command
                                                let _ = controller.send_command(crate::Command::MoveMouse { x, y }).await;
                                                
                                                // Small delay
                                                tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
                                                
                                                // Send ClickMouse command
                                                let _ = controller.send_command(crate::Command::ClickMouse { 
                                                    button: "left".to_string() 
                                                }).await;
                                            }
                                        } else {
                                            mouse_buttons_pressed.remove("left");
                                        }
                                        
                                        // Right click
                                        if win.get_mouse_down(minifb::MouseButton::Right) {
                                            if !mouse_buttons_pressed.contains(&"right") {
                                                println!("🖱️ Right click at ({}, {})", x, y);
                                                mouse_buttons_pressed.insert("right");
                                                
                                                let _ = controller.send_command(crate::Command::MoveMouse { x, y }).await;
                                                tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
                                                let _ = controller.send_command(crate::Command::ClickMouse { 
                                                    button: "right".to_string() 
                                                }).await;
                                            }
                                        } else {
                                            mouse_buttons_pressed.remove("right");
                                        }
                                    }
                                    
                                    // HANDLE KEYBOARD INPUT - SEND COMMANDS
                                    let keys = win.get_keys_pressed(minifb::KeyRepeat::No);
                                    for key in keys {
                                        let key_cmd = match key {
                                            Key::Enter => Some("enter"),
                                            Key::Space => Some("space"),
                                            Key::Backspace => Some("backspace"),
                                            Key::Tab => Some("tab"),
                                            Key::Up => Some("up"),
                                            Key::Down => Some("down"),
                                            Key::Left => Some("left"),
                                            Key::Right => Some("right"),
                                            Key::Delete => Some("delete"),
                                            _ => None,
                                        };
                                        
                                        if let Some(key_name) = key_cmd {
                                            println!("⌨️ Key: {}", key_name);
                                            let _ = controller.send_command(crate::Command::PressKey { 
                                                key: key_name.to_string() 
                                            }).await;
                                        } else {
                                            // Try to handle letter/number keys
                                            let key_str = format!("{:?}", key);
                                            if key_str.len() <= 3 {  // Single character keys like "A", "Key1"
                                                let ch = if key_str.starts_with("Key") && key_str.len() == 4 {
                                                    key_str.chars().nth(3)
                                                } else {
                                                    key_str.chars().next()
                                                };
                                                
                                                if let Some(c) = ch {
                                                    if c.is_alphanumeric() {
                                                        let _ = controller.send_command(crate::Command::TypeText { 
                                                            text: c.to_lowercase().to_string() 
                                                        }).await;
                                                    }
                                                }
                                            }
                                        }
                                    }
                                    
                                    // Update display
                                    if let Err(e) = win.update_with_buffer(&buffer, width as usize, height as usize) {
                                        eprintln!("Failed to update window: {}", e);
                                        break;
                                    }
                                    
                                    if frame_count % 30 == 0 {
                                        println!("📺 Frame #{} | Remote Control Active", frame_count);
                                    }
                                }
                            }
                        }
                    }
                }
            }
            Ok(tokio_tungstenite::tungstenite::Message::Close(_)) => {
                println!("\n✓ Stream closed by agent");
                break;
            }
            Err(e) => {
                eprintln!("WebSocket error: {}", e);
                break;
            }
            _ => {}
        }
    }
    
    println!("\n✓ Remote desktop session ended. Total frames: {}", frame_count);
    drop(window);
    
    Ok(())
}
