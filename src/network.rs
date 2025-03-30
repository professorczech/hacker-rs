// src/network.rs
use anyhow::{anyhow, Context, Result};
use std::process::Command;
use regex::Regex; // Add regex crate to Cargo.toml

// Existing http client function (if any) can remain
// pub fn create_http_client() -> reqwest::Client { ... }

// NEW function to find default gateway
// Returns Ok(Some(ip_string)) or Ok(None) if not found, or Err on execution/parse failure
pub fn get_default_gateway() -> Result<Option<String>> {
    println!("Attempting to find default gateway...");
    #[cfg(windows)]
    {
        // Windows: Use ipconfig and parse
        let output = Command::new("ipconfig")
            .output()
            .context("Failed to execute ipconfig")?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(anyhow!("ipconfig failed with status {}: {}", output.status, stderr));
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        // Regex to find IPv4 Default Gateway line and capture the IP
        // Looks for "Default Gateway", then optional whitespace/dots, then ":", then IP
        let re = Regex::new(r"Default Gateway.*: ([0-9]+\.[0-9]+\.[0-9]+\.[0-9]+)")
                       .expect("Invalid regex"); // Expect should be safe for this pattern

        // Find the first match which is likely the primary gateway
        if let Some(cap) = re.captures(&stdout) {
            if let Some(ip_match) = cap.get(1) {
                let ip = ip_match.as_str().to_string();
                // Basic validation it's not 0.0.0.0 if that appears sometimes
                 if ip != "0.0.0.0" {
                    println!("Found default gateway (Windows): {}", ip);
                    return Ok(Some(ip));
                 }
            }
        }
        println!("Default gateway not found in ipconfig output.");
        Ok(None)
    }
    #[cfg(unix)] // Primarily targeting Linux here
    {
        // Linux: Use `ip route` and parse
        let output = Command::new("ip")
            .args(["route", "show", "default"])
            .output()
            .context("Failed to execute 'ip route show default'")?;

         if !output.status.success() {
             // Might fail if no default route exists
             println!("'ip route show default' failed or no default route found.");
             return Ok(None); // Treat as not found if command fails cleanly
         }

        let stdout = String::from_utf8_lossy(&output.stdout);
         // Regex to find the line starting with "default via" and capture the IP
         let re = Regex::new(r"default via ([0-9]+\.[0-9]+\.[0-9]+\.[0-9]+)")
                        .expect("Invalid regex");

        if let Some(cap) = re.captures(&stdout) {
            if let Some(ip_match) = cap.get(1) {
                let ip = ip_match.as_str().to_string();
                println!("Found default gateway (Linux): {}", ip);
                return Ok(Some(ip));
            }
        }
        println!("Default gateway not found in 'ip route' output.");
        Ok(None)
    }
     #[cfg(not(any(windows, unix)))]
     {
         println!("Default gateway discovery not supported on this platform.");
         Ok(None)
     }
}