use aiem_core::ide;
use clap::Subcommand;

#[derive(Subcommand, Debug)]
pub enum IdeCmd {
    /// List all supported IDE targets (for skills deployment).
    List,
}

pub fn run(cmd: IdeCmd) -> anyhow::Result<()> {
    match cmd {
        IdeCmd::List => {
            println!("{:<14} {:<30} {}", "ID", "DISPLAY NAME", "SKILLS DIR");
            for i in ide::IDES {
                println!("{:<14} {:<30} {}", i.id, i.display_name, i.skills_dir);
            }
        }
    }
    Ok(())
}
