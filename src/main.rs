mod args;
mod cli;
mod command;
mod completions;
mod discovery;
mod frontmatter;
mod help;
mod hooks;
mod new;
mod runner;
mod skill;

fn main() {
    let code = cli::run().unwrap_or_else(|err| {
        eprintln!("las: {err}");
        1
    });
    std::process::exit(code);
}
