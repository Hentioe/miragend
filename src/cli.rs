use clap::Parser;

#[derive(Debug, Parser)]
#[command(
    version,
    about = "Reverse proxy for patching web pages and fighting AI bots"
)]
pub struct Args {}
