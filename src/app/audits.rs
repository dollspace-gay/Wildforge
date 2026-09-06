//! Persisted ledger audits. Each domain retains its own qualification criteria.

use crate::{alchemy, arcane, discovery, dross, implements, materials, planet_atlas, workings};
use std::path::PathBuf;

pub(super) fn alchemy(args: &[String], i: usize) {
    let Some(world) = args.get(i + 1).map(PathBuf::from) else {
        eprintln!("usage: wildforge --alchemy-audit <world>");
        std::process::exit(2);
    };
    match alchemy::audit_world(&world) {
        Ok(audit) => {
            print!("{}", audit.render());
            if !audit.is_qualified() {
                std::process::exit(1);
            }
        }
        Err(error) => {
            eprintln!("alchemy audit failed: {error}");
            std::process::exit(1);
        }
    }
}

pub(super) fn workings(args: &[String], i: usize) {
    let Some(world) = args.get(i + 1).map(PathBuf::from) else {
        eprintln!("usage: wildforge --workings-audit <world>");
        std::process::exit(2);
    };
    match workings::audit_world(&world) {
        Ok(audit) => {
            print!("{}", audit.render());
            if !audit.is_qualified() {
                std::process::exit(1);
            }
        }
        Err(error) => {
            eprintln!("workings audit failed: {error}");
            std::process::exit(1);
        }
    }
}

pub(super) fn implements(args: &[String], i: usize) {
    let Some(world) = args.get(i + 1).map(PathBuf::from) else {
        eprintln!("usage: wildforge --implements-audit <world>");
        std::process::exit(2);
    };
    match implements::audit_world(&world) {
        Ok(audit) => {
            print!("{}", audit.render());
            if !audit.is_qualified() {
                std::process::exit(1);
            }
        }
        Err(error) => {
            eprintln!("implements audit failed: {error}");
            std::process::exit(1);
        }
    }
}

pub(super) fn discovery(args: &[String], i: usize) {
    let Some(world) = args.get(i + 1).map(PathBuf::from) else {
        eprintln!("usage: wildforge --discovery-audit <world>");
        std::process::exit(2);
    };
    match discovery::audit_world(&world) {
        Ok(audit) => {
            print!("{}", audit.render());
            if !audit.is_qualified() {
                std::process::exit(1);
            }
        }
        Err(error) => {
            eprintln!("discovery audit failed: {error}");
            std::process::exit(1);
        }
    }
}

pub(super) fn material(args: &[String], i: usize) {
    let Some(world) = args.get(i + 1).map(PathBuf::from) else {
        eprintln!("usage: wildforge --material-audit <world>");
        std::process::exit(2);
    };
    match materials::audit_world(&world) {
        Ok(audit) => {
            print!("{}", audit.render());
            if !audit.is_balanced() || !audit.is_qualified() {
                std::process::exit(1);
            }
        }
        Err(error) => {
            eprintln!("material audit failed: {error}");
            std::process::exit(1);
        }
    }
}

pub(super) fn arcane(args: &[String], i: usize) {
    let Some(world) = args.get(i + 1).map(PathBuf::from) else {
        eprintln!("usage: wildforge --arcane-audit <world>");
        std::process::exit(2);
    };
    match arcane::audit_world(&world) {
        Ok(audit) => {
            print!("{}", audit.render());
            if !audit.is_balanced() {
                std::process::exit(1);
            }
            match dross::audit_world(&world) {
                Ok(dross) => {
                    print!("{}", dross.render());
                    if !dross.is_balanced() {
                        std::process::exit(1);
                    }
                }
                Err(error) => {
                    eprintln!("dross audit failed: {error}");
                    std::process::exit(1);
                }
            }
        }
        Err(error) => {
            eprintln!("arcane audit failed: {error}");
            std::process::exit(1);
        }
    }
}

pub(super) fn water(args: &[String], i: usize) {
    let Some(world) = args.get(i + 1).map(PathBuf::from) else {
        eprintln!("usage: wildforge --water-audit <world>");
        std::process::exit(2);
    };
    match planet_atlas::PlanetAtlas::load(&world) {
        Ok(atlas) => {
            print!("{}", atlas.water_audit_text());
            let audit = atlas.water_audit();
            if audit.unexplained_water_delta_hu != 0 || audit.unexplained_salt_delta != 0 {
                std::process::exit(1);
            }
        }
        Err(error) => {
            eprintln!("water audit failed: {error}");
            std::process::exit(1);
        }
    }
}
