mod args;
mod cli;
mod command;
mod completions;
mod discovery;
mod frontmatter;
mod help;
mod history;
mod hooks;
mod json;
mod new;
mod runner;
mod skill;
mod sync;

fn main() {
    let code = cli::run().unwrap_or_else(|err| {
        eprintln!("las: {err}");
        1
    });
    std::process::exit(code);
}
