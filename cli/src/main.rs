//! `winterwallet` CLI — host interface for the WinterWallet program.

mod commands;
mod helpers;
mod state;

use clap::{Parser, Subcommand};

/// Default SPL Token program ID.
const TOKEN_PROGRAM: &str = "TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA";

#[derive(Parser)]
#[command(name = "winterwallet", version, about = "WinterWallet CLI")]
struct Cli {
    /// Solana RPC URL.
    #[arg(long, global = true, default_value = "http://127.0.0.1:8899")]
    rpc_url: String,

    /// Path to the fee-payer keypair file for transaction commands.
    #[arg(long, global = true)]
    keypair: Option<String>,

    /// Emit machine-readable JSON.
    #[arg(long, global = true)]
    json: bool,

    /// Build and validate the operation without signing or sending.
    #[arg(long, global = true)]
    dry_run: bool,

    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Generate a new 24-word mnemonic and display wallet ID + PDA.
    Create,

    /// Initialize a new WinterWallet on-chain.
    Init,

    /// Withdraw lamports from the wallet via Advance(Withdraw).
    Withdraw {
        /// Receiver address (base58).
        #[arg(long)]
        to: String,

        /// Amount in lamports.
        #[arg(long)]
        amount: u64,
    },

    /// Transfer SPL tokens from the wallet via Advance(TokenTransfer).
    Transfer {
        /// Recipient owner address (base58). ATA is derived automatically.
        #[arg(long)]
        to: String,

        /// Token mint address (base58).
        #[arg(long)]
        mint: String,

        /// Amount in token base units.
        #[arg(long)]
        amount: u64,

        /// Token program ID. Defaults to SPL Token. Use Token-2022 address for Token-2022 mints.
        #[arg(long, default_value = TOKEN_PROGRAM)]
        token_program: String,
    },

    /// Recover wallet position from mnemonic + on-chain state.
    Recover {
        /// Maximum child positions to scan.
        #[arg(long, default_value = "10000")]
        max_depth: u32,
    },

    /// Display wallet info: ID, PDA, balance, root, position.
    Info,
}

fn main() {
    let cli = Cli::parse();

    let result = match &cli.command {
        Command::Create => commands::create::run(cli.json),
        Command::Init => required_keypair(&cli)
            .and_then(|keypair| commands::init::run(keypair, &cli.rpc_url, cli.json, cli.dry_run)),
        Command::Withdraw { to, amount } => required_keypair(&cli).and_then(|keypair| {
            commands::withdraw::run(keypair, to, *amount, &cli.rpc_url, cli.json, cli.dry_run)
        }),
        Command::Transfer {
            to,
            mint,
            amount,
            token_program,
        } => required_keypair(&cli).and_then(|keypair| {
            commands::transfer::run(commands::transfer::RunArgs {
                keypair_path: keypair,
                to,
                mint,
                amount: *amount,
                rpc_url: &cli.rpc_url,
                token_program,
                json_output: cli.json,
                dry_run: cli.dry_run,
            })
        }),
        Command::Recover { max_depth } => {
            commands::recover::run(&cli.rpc_url, *max_depth, cli.json)
        }
        Command::Info => commands::info::run(&cli.rpc_url, cli.json),
    };

    if let Err(e) = result {
        eprintln!("error: {e}");
        std::process::exit(1);
    }
}

fn required_keypair(cli: &Cli) -> Result<&str, String> {
    cli.keypair
        .as_deref()
        .ok_or_else(|| "--keypair is required for this command".to_string())
}
