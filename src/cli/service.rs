use console::style;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

pub fn handle_service_command(
    action: super::ServiceAction,
) -> Result<(), Box<dyn std::error::Error>> {
    match action {
        super::ServiceAction::Install => install_service(),
        super::ServiceAction::Uninstall => uninstall_service(),
    }
}

fn install_service() -> Result<(), Box<dyn std::error::Error>> {
    let exe_path = env::current_exe()?;
    let config_path = crate::cli::find_existing_config()
        .ok_or("No configuration file found. Please run `tokenscavenger setup` first.")?;
    let config_path = fs::canonicalize(config_path)?;

    if cfg!(target_os = "macos") {
        install_macos_service(&exe_path, &config_path)
    } else if cfg!(target_os = "linux") {
        install_linux_service(&exe_path, &config_path)
    } else {
        Err("Automatic service installation is only supported on macOS and Linux.".into())
    }
}

fn install_macos_service(
    exe_path: &Path,
    config_path: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    let home = env::var("HOME")?;
    let plist_path =
        PathBuf::from(&home).join("Library/LaunchAgents/com.tokenscavenger.server.plist");

    let plist_content = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>com.tokenscavenger.server</string>
    <key>ProgramArguments</key>
    <array>
        <string>{}</string>
        <string>--config</string>
        <string>{}</string>
    </array>
    <key>RunAtLoad</key>
    <true/>
    <key>KeepAlive</key>
    <true/>
    <key>StandardOutPath</key>
    <string>{}/Library/Logs/tokenscavenger.log</string>
    <key>StandardErrorPath</key>
    <string>{}/Library/Logs/tokenscavenger.err</string>
</dict>
</plist>"#,
        exe_path.display(),
        config_path.display(),
        home,
        home
    );

    if let Some(parent) = plist_path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(&plist_path, plist_content)?;

    println!(
        "{} Created launchd plist at: {}",
        style("✓").green(),
        plist_path.display()
    );

    // Load the service
    let output = Command::new("launchctl")
        .arg("load")
        .arg(&plist_path)
        .output()?;

    if output.status.success() {
        println!("{} Service loaded and started.", style("✓").green());
    } else {
        println!(
            "{} Service created but failed to load. You may need to run:",
            style("!").yellow()
        );
        println!("  launchctl load {}", plist_path.display());
    }

    Ok(())
}

fn install_linux_service(
    exe_path: &Path,
    config_path: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    let user = env::var("USER")?;
    let service_content = format!(
        r#"[Unit]
Description=TokenScavenger LLM Router
After=network.target

[Service]
ExecStart={} --config {}
Restart=always
User={}

[Install]
WantedBy=multi-user.target
"#,
        exe_path.display(),
        config_path.display(),
        user
    );

    let service_path = PathBuf::from("/etc/systemd/system/tokenscavenger.service");

    println!(
        "{} To install the systemd service, run the following commands:",
        style("ℹ").blue()
    );
    println!(
        "\ncat <<EOF | sudo tee {}\n{}EOF\n",
        service_path.display(),
        service_content
    );
    println!("sudo systemctl daemon-reload");
    println!("sudo systemctl enable tokenscavenger");
    println!("sudo systemctl start tokenscavenger");

    Ok(())
}

fn uninstall_service() -> Result<(), Box<dyn std::error::Error>> {
    if cfg!(target_os = "macos") {
        let home = env::var("HOME")?;
        let plist_path =
            PathBuf::from(&home).join("Library/LaunchAgents/com.tokenscavenger.server.plist");

        if plist_path.exists() {
            Command::new("launchctl")
                .arg("unload")
                .arg(&plist_path)
                .status()
                .ok();
            fs::remove_file(&plist_path)?;
            println!("{} Uninstalled macOS service.", style("✓").green());
        } else {
            println!("{} No macOS service found to uninstall.", style("ℹ").blue());
        }
    } else if cfg!(target_os = "linux") {
        println!(
            "{} To uninstall the systemd service, run:",
            style("ℹ").blue()
        );
        println!("  sudo systemctl stop tokenscavenger");
        println!("  sudo systemctl disable tokenscavenger");
        println!("  sudo rm /etc/systemd/system/tokenscavenger.service");
        println!("  sudo systemctl daemon-reload");
    }

    Ok(())
}
