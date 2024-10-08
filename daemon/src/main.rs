use clap::{Parser, Subcommand};
use ethersync::connect::PeerConnectionInfo;
use ethersync::{daemon::Daemon, logging, sandbox};
use std::io;
use std::path::{Path, PathBuf};
use tokio::signal;
use tracing::{error, info};

mod jsonrpc_forwarder;

const DEFAULT_SOCKET_PATH: &str = "/tmp/ethersync";
const DEFAULT_PORT: &str = "4242";
const ETHERSYNC_CONFIG_DIR: &str = ".ethersync";

#[derive(Parser)]
#[command(version, about, long_about = None)]
#[command(propagate_version = true)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
    /// Path to the Unix domain socket to use for communication between daemon and editors.
    #[arg(short, long, global = true, default_value = DEFAULT_SOCKET_PATH)]
    socket_path: PathBuf,
    /// Enable verbose debug output.
    #[arg(short, long, global = true, action)]
    debug: bool,
}

#[derive(Subcommand)]
enum Commands {
    /// Launch Ethersync's background process that connects with clients and other nodes.
    Daemon {
        /// Port to listen on as a hosting peer.
        #[arg(short, long, default_value = DEFAULT_PORT)]
        port: u16,
        /// The directory to sync. Defaults to current directory.
        directory: Option<PathBuf>,
        /// IP + port of a peer to connect to. Example: 192.168.1.42:1234
        #[arg(long)]
        peer: Option<String>,
        /// Initialize the current contents of the directory as a new Ethersync directory.
        #[arg(long)]
        init: bool,
    },
    /// Open a JSON-RPC connection to the Ethersync daemon on stdin/stdout.
    Client,
}

fn has_ethersync_directory(dir: &Path) -> bool {
    let ethersync_dir = dir.join(ETHERSYNC_CONFIG_DIR);
    // Using the sandbox method here is technically unnecessary,
    // but we want to really run all path operations through the sandbox module.
    sandbox::exists(dir, &ethersync_dir).expect("Failed to check") && ethersync_dir.is_dir()
}

#[tokio::main]
async fn main() -> io::Result<()> {
    let default_panic = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        default_panic(info);
        std::process::exit(1);
    }));

    let cli = Cli::parse();

    logging::initialize(cli.debug);

    let socket_path = cli.socket_path;

    match cli.command {
        Commands::Daemon {
            port,
            directory,
            peer,
            init,
        } => {
            let directory = directory
                .unwrap_or_else(|| {
                    std::env::current_dir().expect("Could not access current directory")
                })
                .canonicalize()
                .expect("Could not access given directory");
            if !has_ethersync_directory(&directory) {
                error!(
                    "No {} found in {} (create it to Ethersync-enable the directory)",
                    ETHERSYNC_CONFIG_DIR,
                    directory.display()
                );
                return Ok(());
            }
            let peer_connection_info = if let Some(peer) = peer {
                PeerConnectionInfo::Connect(peer)
            } else {
                PeerConnectionInfo::Accept(port)
            };
            info!("Starting Ethersync on {}", directory.display());
            Daemon::new(peer_connection_info, &socket_path, &directory, init);
            match signal::ctrl_c().await {
                Ok(()) => {}
                Err(err) => {
                    eprintln!("Unable to listen for shutdown signal: {err}");
                    // still shut down.
                }
            }
        }
        Commands::Client => {
            jsonrpc_forwarder::connection(&socket_path);
        }
    }
    Ok(())
}
