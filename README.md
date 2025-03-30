# Hacker-RS

A Rust-powered cybersecurity assistant leveraging local AI models through Ollama. Designed for penetration testers and security researchers to execute complex security tasks through natural language prompts.

![CLI Demo](https://via.placeholder.com/800x400.png?text=CLI+Demo+GIF+Placeholder)

## Features

- 🚀 Local AI processing with Ollama integration
- 🔧 Automated tool dependency management
- ⚡ Async command execution with Tokio
- 🔒 Context-aware command chaining
- 📁 Output saving and session persistence
- 🖥️ Cross-platform support (Linux/Windows/macOS)

## Prerequisites

- [Rust](https://www.rust-lang.org/tools/install) 1.65+
- [Ollama](https://ollama.ai/) running locally
- Common security tools (auto-installed):
  - Nmap
  - SET (Social Engineer Toolkit)
  - Metasploit (optional)

## Installation

```bash
# Clone repository
git clone https://github.com/yourusername/hacker-rs.git
cd hacker-rs

# Build with cargo
cargo build --release

# Install system-wide (optional)
sudo cp target/release/hacker-rs /usr/local/bin/


# Basic command execution
hacker-rs run "Perform network reconnaissance on 192.168.1.0/24"

# Save output to file
hacker-rs run "Scan for SQL vulnerabilities" -o scan_results.txt

# Interactive session (Coming soon!)
hacker-rs interactive

# Use custom config
hacker-rs --config ~/custom_config.toml run "Analyze firewall rules"

Supported Tools
The assistant automatically installs missing dependencies:

Tool	Linux	Windows
Nmap	✅ Auto-install	❌ Limited
SET	✅ Auto-install	⚠️ WSL Only
Metasploit	✅ Manual	❌
Wireshark	✅ Auto-install	✅ Chocolatey


# Troubleshooting
Ollama Connection Issues:

# Verify Ollama service status
ollama serve

# Check firewall rules
sudo ufw allow 11434/tcp