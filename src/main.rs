#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

mod cli;
mod launcher;
mod ui;

fn main() -> eframe::Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.first().map(String::as_str) == Some("--cli") {
        attach_parent_console();
        std::process::exit(cli::run(&args[1..]));
    }
    let startup = match launcher::selfupdate::on_startup(&args) {
        launcher::selfupdate::Startup::Exit => return Ok(()),
        launcher::selfupdate::Startup::Run { installed, updated_from } => ui::app::StartupInfo { installed, updated_from },
    };

    let icon = image::load_from_memory(include_bytes!("../assets/icon.png"))
        .ok()
        .map(|img| {
            let img = img.to_rgba8();
            egui::IconData { width: img.width(), height: img.height(), rgba: img.into_raw() }
        });

    let mut viewport = egui::ViewportBuilder::default()
        .with_title("Akulav Launcher")
        .with_app_id("akulav-launcher")
        .with_inner_size([1180.0, 720.0])
        .with_min_inner_size([960.0, 600.0])
        .with_decorations(false)
        .with_resizable(true);
    if let Some(icon) = icon {
        viewport = viewport.with_icon(icon);
    }

    let options = eframe::NativeOptions { viewport, centered: true, ..Default::default() };
    eframe::run_native("Akulav Launcher", options, Box::new(move |cc| Ok(Box::new(ui::app::LauncherApp::new(cc, startup)))))
}

/// Release builds use the GUI subsystem; reattach to the terminal so `--cli` output is visible.
#[cfg(windows)]
fn attach_parent_console() {
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn AttachConsole(pid: u32) -> i32;
    }
    const ATTACH_PARENT_PROCESS: u32 = u32::MAX;
    // SAFETY: plain Win32 call with a constant argument.
    unsafe {
        AttachConsole(ATTACH_PARENT_PROCESS);
    }
}

#[cfg(not(windows))]
fn attach_parent_console() {}
