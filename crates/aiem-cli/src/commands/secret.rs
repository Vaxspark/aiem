use aiem_core::secrets::Vault;
use clap::Subcommand;

#[derive(Subcommand, Debug)]
pub enum SecretCmd {
    /// Store a secret value in the OS keyring.
    ///
    /// The value is read from `--value`, `--stdin`, or interactively (hidden).
    Set {
        name: String,
        /// Value on the command line (not recommended — shows in history).
        #[arg(long)]
        value: Option<String>,
        /// Read the value from stdin instead of prompting.
        #[arg(long)]
        stdin: bool,
        /// Optional human-readable description.
        #[arg(long)]
        description: Option<String>,
    },
    /// Print the stored value of a secret (sensitive — avoid piping to logs).
    Get { name: String },
    /// List all secrets (names and metadata only — values stay in the keyring).
    List,
    /// Delete a secret from both the keyring and aiem's index.
    #[command(alias = "rm")]
    Remove { name: String },
}

pub fn run(cmd: SecretCmd) -> anyhow::Result<()> {
    match cmd {
        SecretCmd::Set { name, value, stdin, description } => {
            let v = if let Some(v) = value {
                v
            } else if stdin {
                use std::io::Read;
                let mut buf = String::new();
                std::io::stdin().read_to_string(&mut buf)?;
                buf.trim_end_matches(&['\n', '\r'][..]).to_string()
            } else {
                rpassword::prompt_password(format!("value for `{name}`: "))?
            };
            let mut vault = Vault::load()?;
            vault.set(&name, &v, description)?;
            println!("✓ stored `{name}` in OS keyring");
        }
        SecretCmd::Get { name } => {
            let vault = Vault::load()?;
            let v = vault.get(&name)?;
            println!("{v}");
        }
        SecretCmd::List => {
            let vault = Vault::load()?;
            if vault.is_empty() {
                println!("no secrets stored");
                return Ok(());
            }
            println!("{:<24}  {:<20}  description", "name", "updated");
            for name in vault.names() {
                let meta = vault.meta(name).cloned().unwrap_or_default();
                let desc = meta.description.as_deref().unwrap_or("");
                println!(
                    "{:<24}  {:<20}  {}",
                    name,
                    meta.updated_at.format("%Y-%m-%d %H:%M"),
                    desc
                );
            }
        }
        SecretCmd::Remove { name } => {
            let mut vault = Vault::load()?;
            vault.delete(&name)?;
            println!("✓ removed `{name}`");
        }
    }
    Ok(())
}
