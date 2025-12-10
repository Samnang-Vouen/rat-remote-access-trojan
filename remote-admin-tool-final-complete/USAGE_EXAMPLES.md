# 📖 Complete Usage Examples

## 🎯 All 22 Commands with Examples

### 1. Ping - Test Connection
```bash
> ping
✓ Pong! Agent is alive.
```

### 2. Execute Commands
```bash
# Windows
> exec dir
> exec ipconfig
> exec tasklist

# Linux/Mac
> exec ls -la
> exec ps aux
> exec df -h
```

### 3. System Information
```bash
> sysinfo
✓ System information
System: Windows 10
Kernel: 10.0.19045
OS Version: Windows 10 Pro
Hostname: DESKTOP-ABC123
CPUs: 8
Total Memory: 16384 MB
Used Memory: 8192 MB
Total Swap: 4096 MB
```

### 4. List Processes
```bash
> processes
✓ Found 234 processes
PID: 1234 | Name: chrome.exe | CPU: 5.2% | Memory: 256 MB
PID: 5678 | Name: Code.exe | CPU: 2.1% | Memory: 512 MB
```

### 5. Screenshot
```bash
> screenshot
✓ Screenshot captured
Image saved as 'screenshot.png'
```

### 6. List Files
```bash
> filelist C:\Users\username\Desktop
✓ Found 10 items in C:\Users\username\Desktop
FILE |         1234 bytes | document.txt
DIR  |         4096 bytes | Projects
FILE |       524288 bytes | photo.jpg
```

### 7. Webcam Capture
```bash
> webcam
Enter duration in seconds (default 3): 5
Requesting webcam capture for 5 seconds...
✓ Real webcam captured after 5 seconds!
Image saved as 'webcam.png'
```

### 8. Download File/Folder 🆕
```bash
# Download single file
> download C:\Users\username\report.pdf
Requesting download: C:\Users\username\report.pdf
✓ FILE|report.pdf|125000

[FILE Data - Base64 Encoded]
Suggested name: report.pdf
Size: 125000 bytes
Enter filename to save as (default: report.pdf): my-report.pdf
✓ Saved as 'my-report.pdf'

# Download entire folder (auto ZIP)
> download C:\Users\username\Documents\Projects
Requesting download: C:\Users\username\Documents\Projects
✓ FOLDER|Projects.zip|2500000

[FOLDER Data - Base64 Encoded]
Suggested name: Projects.zip
Size: 2500000 bytes
Enter filename to save as (default: Projects.zip): 
✓ Saved as 'Projects.zip'
💡 Tip: Extract with: unzip Projects.zip
```

### 9. Upload File 🆕
```bash
> upload
Enter local file path to upload: C:\Users\me\presentation.pptx
Read 1500000 bytes from local file
Enter remote path to save as: C:\Users\agent\Desktop\presentation.pptx
Uploading 1500000 bytes to C:\Users\agent\Desktop\presentation.pptx...
✓ File uploaded successfully: C:\Users\agent\Desktop\presentation.pptx (1500000 bytes)

# Verify it uploaded
> filelist C:\Users\agent\Desktop
FILE |      1500000 bytes | presentation.pptx  ← File is there!
```

### 10. Start Webcam Stream
```bash
> livestream
Starting live webcam stream...
Enter WebSocket port (default 9001): 9001
✓ Live stream started! Connect WebSocket client to port 9001
Enter agent IP address (e.g., 127.0.0.1): 192.168.1.100

Starting webcam stream viewer...
✓ Connected! Receiving stream...

[LIVE] Frame:    145 | FPS:  29.8 | Data:   4.52 MB | Bandwidth:  1.23 Mbps
[LIVE] Frame:    175 | FPS:  30.1 | Data:   5.40 MB | Bandwidth:  1.25 Mbps

Press Ctrl+C to stop
^C
╔══════════════════════════════════════╗
║       Stream Summary                 ║
╚══════════════════════════════════════╝
Total frames received: 894
Total data: 27.50 MB
Duration: 29.8 seconds
Average FPS: 30.0

Frames saved to: stream_frames/
Create video: ffmpeg -framerate 30 -i stream_frames/frame_%06d.jpg output.mp4
```

### 11. Stop Webcam Stream
```bash
> stopstream
Stopping webcam stream...
✓ Live stream stopped
```

### 12. Start Screen Monitoring
```bash
> screenstream
Starting screen monitoring...
Enter WebSocket port (default 9002): 9002
✓ Screen monitoring started! Connect to port 9002
Enter agent IP address (e.g., 127.0.0.1): 192.168.1.100

Starting screen monitor viewer...
Press Ctrl+C to stop
✓ Connected! Receiving stream...

[LIVE SCREEN] Frame:    25 | FPS:  5.0 | Data:   8.75 MB | Bandwidth:  0.89 Mbps
[LIVE SCREEN] Frame:    50 | FPS:  5.0 | Data:  17.50 MB | Bandwidth:  0.90 Mbps

Press Ctrl+C to stop
```

### 13. Stop Screen Monitoring
```bash
> stopscreen
Stopping screen monitoring...
✓ Screen monitoring stopped
```

### 14. Move Mouse
```bash
> movemouse
Enter X coordinate: 500
Enter Y coordinate: 300
⚠️  Moving mouse to (500, 300)
✓ Mouse moved to (500, 300)
```

### 15. Click Mouse
```bash
> click
Enter button (left/right/middle): left
⚠️  Clicking mouse: left
✓ Mouse clicked: left

# Right click
> click
Enter button (left/right/middle): right
⚠️  Clicking mouse: right
✓ Mouse clicked: right
```

### 16. Type Text
```bash
> type Hello, this is a test!
⚠️  Typing text: 23 characters
✓ Typed 23 characters
```

### 17. Press Key
```bash
# Press Enter
> presskey enter
⚠️  Pressing key: enter
✓ Pressed key: enter

# Press Escape
> presskey esc
⚠️  Pressing key: esc
✓ Pressed key: esc

# Supported keys: enter, esc, tab, space, backspace, delete,
#                 up, down, left, right, home, end, pageup, pagedown,
#                 f1-f12, ctrl, alt, shift, win
```

### 18. Open Application
```bash
# Windows
> openapp notepad
⚠️  Opening: notepad
✓ Opened: notepad

> openapp calc
✓ Opened: calc

# Full path
> openapp C:\Program Files\Application\app.exe
✓ Opened: C:\Program Files\Application\app.exe
```

### 19. Get Mouse Position
```bash
> mousepos
Getting mouse position...
✓ Mouse position: (1024, 768)
Mouse position: (1024, 768)
```

### 20. Shutdown Agent
```bash
> shutdown
Are you sure you want to shutdown the agent? (yes/no): yes
✓ Agent shutting down...
Connection will close...

╔═══════════════════════════════════════════╗
║  Agent Shutdown Complete                  ║
║  Goodbye!                                 ║
╚═══════════════════════════════════════════╝
```

### 21. Help
```bash
> help
[Shows complete menu]
```

### 22. Quit
```bash
> quit
Exiting controller...
```

---

## 🎬 Complete Workflow Examples

### Example 1: Remote File Management
```bash
# List files
> filelist C:\Users\agent\Documents

# Download a file
> download C:\Users\agent\Documents\report.pdf
✓ Saved as 'report.pdf'

# Upload a modified version
> upload
Enter local: C:\Users\me\report-edited.pdf
Enter remote: C:\Users\agent\Documents\report-edited.pdf
✓ File uploaded successfully

# Verify
> filelist C:\Users\agent\Documents
```

### Example 2: Backup Folder
```bash
# Download entire folder as ZIP
> download C:\Users\agent\Documents\Projects
✓ FOLDER|Projects.zip|5000000
✓ Saved as 'Projects.zip'

# Extract locally
$ unzip Projects.zip
```

### Example 3: Remote Automation
```bash
# Open notepad
> openapp notepad

# Wait a moment, then type
> type This is automated text

# Press Enter
> presskey enter

# Type more
> type Line 2 of text

# Save with Ctrl+S (need to press keys)
> presskey ctrl
> presskey s
```

### Example 4: Screen Monitoring Session
```bash
# Start screen monitoring
> screenstream
[Monitoring at 5 FPS]

# Stop after observing
Press Ctrl+C

# Create video from captured frames
$ ffmpeg -framerate 5 -i stream_frames/frame_%06d.jpg screen-recording.mp4
```

### Example 5: System Diagnostics
```bash
# Get system info
> sysinfo

# Check running processes
> processes

# Execute diagnostic command
> exec systeminfo

# Take screenshot of desktop
> screenshot

# Download diagnostic logs
> download C:\Windows\Logs\CBS\CBS.log
```

---

## 🔥 Advanced Usage

### Batch Operations
```bash
# Download multiple files
> download C:\file1.txt
> download C:\file2.txt
> download C:\file3.txt

# Upload multiple files
> upload
# [Enter file1 info]
> upload
# [Enter file2 info]
```

### Remote Installation
```bash
# Upload installer
> upload
Enter local: C:\installers\app-setup.exe
Enter remote: C:\Users\agent\Desktop\app-setup.exe
✓ Uploaded

# Run installer
> exec C:\Users\agent\Desktop\app-setup.exe /S
✓ Command executed
```

### Automated Testing
```bash
# Open browser
> openapp chrome.exe

# Wait, then type URL
> type https://example.com

# Press Enter
> presskey enter

# Take screenshot
> screenshot
```

---

## 📝 Tips and Tricks

### 1. Use Full Paths
Always use complete file paths to avoid errors:
```bash
✅ Good: download C:\Users\agent\Desktop\file.txt
❌ Bad:  download file.txt
```

### 2. Check Before Download
List files first to ensure they exist:
```bash
> filelist C:\Users\agent\Desktop
# Find the file you want
> download C:\Users\agent\Desktop\found-file.txt
```

### 3. Test Commands First
Use `exec` to test before automating:
```bash
> exec dir C:\test
# Verify path exists
> download C:\test\file.txt
```

### 4. Monitor Progress
Large downloads show progress through size:
```bash
FOLDER|BigProject.zip|50000000  ← 50MB folder
```

### 5. Verify Uploads
After upload, list files to confirm:
```bash
> upload
# [Upload file]
> filelist [target directory]
# Verify file appears
```

---

## ⚡ Performance Tips

### Fast Downloads
- Single files: Instant (<1MB)
- Folders: Depends on size, ZIP compression helps

### Fast Uploads
- Small files: <1 second
- Large files: ~1-2 MB/s over network

### Streaming
- Webcam: 30 FPS, ~1-2 Mbps
- Screen: 5 FPS, ~0.5-1 Mbps

---

**All examples tested and working!** 🎉
