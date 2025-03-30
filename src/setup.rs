// src/setup.rs
use anyhow::{anyhow, Context, Result};
use directories_next::UserDirs;
use os_info::Type;
use std::fmt; // Import fmt for Display trait
use sysinfo::System;
use std::path::PathBuf;
use std::process::Command;
use which::which;

// Derive Clone, Debug, and add Display
#[derive(Clone, Debug)] // Removed Serialize/Deserialize for now unless needed
pub enum Platform {
    KaliLinux,
    Windows,
    OtherLinux,
    Unsupported,
}

// Implement Display trait for easy conversion to string
impl fmt::Display for Platform {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Platform::KaliLinux => write!(f, "Kali Linux"),
            Platform::Windows => write!(f, "Windows"),
            Platform::OtherLinux => write!(f, "Linux (Other)"),
            Platform::Unsupported => write!(f, "Unsupported OS"),
        }
    }
}

pub struct SystemSetup {
    // Make platform public
    pub platform: Platform, // <-- Changed to pub
    is_admin: bool,         // Keep is_admin private for now
}

impl SystemSetup {
    pub fn new() -> Self {
        let sys = System::new_all();
        let platform = detect_platform(&sys);
        let is_admin = is_elevated();

        SystemSetup { platform, is_admin }
    }
    // ... rest of SystemSetup impl remains the same ...

    async fn install_ollama_linux(&self) -> Result<()> {
        let install_script = reqwest::get("https://ollama.ai/install.sh")
            .await?
            .text()
            .await?;

        let mut cmd = if self.is_admin {
            Command::new("sh")
        } else {
            let mut cmd = Command::new("sudo");
            cmd.arg("sh");
            cmd
        };

        cmd.arg("-c").arg(&install_script);
        let status = cmd.status()?;

        if status.success() {
            self.enable_ollama_service().await
        } else {
            Err(anyhow!("Failed to install Ollama"))
        }
    }

    pub async fn ensure_ollama(&self) -> Result<()> {
        if self.check_ollama_installed().await? {
            return Ok(());
        }

        match self.platform {
            Platform::KaliLinux | Platform::OtherLinux => self.install_ollama_linux().await,
            Platform::Windows => self.install_ollama_windows().await,
            _ => Err(anyhow!(
                "Unsupported platform for automatic Ollama installation"
            )),
        }
    }

    async fn enable_ollama_service(&self) -> Result<()> {
        let status = if self.is_admin {
            Command::new("systemctl")
                .args(&["enable", "--now", "ollama"])
                .status()?
        } else {
            Command::new("sudo")
                .args(&["systemctl", "enable", "--now", "ollama"])
                .status()?
        };

        if status.success() {
            Ok(())
        } else {
            Err(anyhow!("Failed to enable Ollama service"))
        }
    }

    async fn check_ollama_installed(&self) -> Result<bool> {
        let status = Command::new("ollama")
            .arg("--version")
            .status()
            .map_err(|_| anyhow::anyhow!("Ollama not found"))?;

        Ok(status.success())
    }

    async fn install_ollama_windows(&self) -> Result<()> {
        let path = UserDirs::new()
            .context("Failed to find user directories")?
            .download_dir()
            .map(PathBuf::from)
            .context("Failed to find downloads directory")?
            .join("OllamaSetup.exe");

        let client = reqwest::Client::new();
        let response = client
            .get("https://ollama.com/download/OllamaSetup.exe")
            .send()
            .await?;

        let mut file = std::fs::File::create(&path)?;
        let content = response.bytes().await?;
        std::io::copy(&mut content.as_ref(), &mut file)?;

        let status = Command::new("cmd")
            .args(&["/C", "start", "/wait", path.to_str().unwrap()])
            .status()?;

        if status.success() {
            Ok(())
        } else {
            Err(anyhow!("Failed to install Ollama on Windows"))
        }
    }

    pub async fn check_and_install_tool(&self, tool: &str) -> Result<()> {
        if which(tool).is_ok() {
            return Ok(());
        }

        match self.platform {
            Platform::KaliLinux => self.apt_install(tool).await,
            Platform::Windows => self.winget_install(tool).await,
            _ => Err(anyhow::anyhow!(
                "Automatic installation not supported for this platform"
            )),
        }
    }

    async fn apt_install(&self, package: &str) -> Result<()> {
        let mut cmd = if self.is_admin {
            Command::new("apt")
        } else {
            Command::new("sudo")
        };

        cmd.args(&["install", "-y", package]);
        let status = cmd.status()?;

        if status.success() {
            Ok(())
        } else {
            Err(anyhow!("Failed to install {}", package))
        }
    }

    async fn winget_install(&self, package: &str) -> Result<()> {
        let status = Command::new("winget")
            .args(&[
                "install",
                "--silent",
                "--accept-package-agreements",
                package,
            ])
            .status()
            .map_err(|_| anyhow!("winget not found - requires Windows 10 1709+"))?;

        if status.success() {
            Ok(())
        } else {
            Err(anyhow!("Failed to install {} via winget", package))
        }
    }
}


// --- detect_platform function (no changes) ---
fn detect_platform(_sys: &System) -> Platform {
    let info = os_info::get();

    match info.os_type() {
        Type::Kali => Platform::KaliLinux,
        Type::Windows => Platform::Windows,
        Type::Linux => Platform::OtherLinux,
        _ => Platform::Unsupported,
    }
}

// --- is_elevated function (no changes from last correct version) ---
fn is_elevated() -> bool {
    #[cfg(windows)]
    {
        use winapi::ctypes::c_void;
        use winapi::um::processthreadsapi::{GetCurrentProcess, OpenProcessToken};
        use winapi::um::securitybaseapi::GetTokenInformation;
        use winapi::um::winnt::{TokenElevation, TOKEN_ELEVATION, TOKEN_QUERY};

        unsafe {
            let mut token = std::ptr::null_mut();
            if OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) == 0 {
                return false;
            }

            let mut elevation = TOKEN_ELEVATION { TokenIsElevated: 0 };
            let mut size = std::mem::size_of::<TOKEN_ELEVATION>() as u32;

            if GetTokenInformation(
                token,
                TokenElevation,
                (&mut elevation as *mut TOKEN_ELEVATION).cast::<c_void>(),
                size,
                &mut size,
            ) == 0
            {
                winapi::um::handleapi::CloseHandle(token);
                return false;
            }

            winapi::um::handleapi::CloseHandle(token);
            elevation.TokenIsElevated != 0
        }
    }

    #[cfg(unix)]
    {
        // Check root user (UID 0) for Unix systems
        // Ensure libc crate is a dependency
        unsafe { libc::geteuid() == 0 }
    }

    #[cfg(not(any(windows, unix)))]
    {
        false
    }
}