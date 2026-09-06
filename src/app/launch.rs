//! Runtime launch adapters. Agent control uses ordinary guest admission.

use crate::{agent, dedicated};

pub(super) fn server(args: &[String], i: usize) {
    let world = args
        .get(i + 1)
        .cloned()
        .unwrap_or_else(|| "world1".to_string());
    dedicated::run_headless_server(&world);
}

pub(super) fn agent(args: &[String], i: usize) {
    let addr = args.get(i + 1).cloned().unwrap_or_else(|| {
        eprintln!("usage: wildforge --agent <host[:port]> [--name NAME]");
        std::process::exit(2);
    });
    let name = args
        .iter()
        .position(|a| a == "--name")
        .and_then(|n| args.get(n + 1).cloned())
        .unwrap_or_else(|| "AGENT".to_string());
    agent::run_agent(&addr, &name);
}
