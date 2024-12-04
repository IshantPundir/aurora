static POSSIBLE_BACKENDS: &[&str] = &[
    #[cfg(feature = "winit")]
    "--winit : Run aurora as a X11 or Wayland client using winit.",
    #[cfg(feature = "udev")]
    "--tty-udev : Run aurora as a tty udev client (requires root if without logind).",
];

fn main() {
    if let Ok(env_filter) = tracing_subscriber::EnvFilter::try_from_default_env() {
        tracing_subscriber::fmt()
            .compact()
            .with_env_filter(env_filter)
            .init();
    } else {
        tracing_subscriber::fmt().compact().init();
    }

    profiling::register_thread!("Main Thread");

    let arg = ::std::env::args().nth(1);
    match arg.as_ref().map(|s| &s[..]) {
        #[cfg(feature = "winit")]
        Some("--winit") => {
            tracing::info!("Starting aurora with winit backend");
            aurora::winit::run_winit();
        }
        #[cfg(feature = "udev")]
        Some("--tty-udev") => {
            tracing::info!("Starting aurora on a tty using udev");
            aurora::udev::run_udev();
        }
        Some(other) => {
            tracing::error!("Unknown backend: {}", other);
        }
        None => {
            #[allow(clippy::disallowed_macros)]
            {
                println!("USAGE: aurora --backend");
                println!();
                println!("Possible backends are:");
                for b in POSSIBLE_BACKENDS {
                    println!("\t{}", b);
                }
            }
        }
    }
}