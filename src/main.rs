#![feature(ip)]
mod app;
mod device;
mod eap;
mod logger;
mod service;
mod settings;
mod socket;
mod udp;
mod util;

use anyhow::Result;

use crate::settings::Command;
use crate::util::ShutdownSignal;

fn main() -> Result<()> {
    match settings::parse_cli()? {
        Command::Run(run) => app::run(run.settings, run.debug, run.mode, ShutdownSignal::new()),
        Command::RunService => service::dispatch(),
        Command::Service(command) => service::handle(command),
    }
}
