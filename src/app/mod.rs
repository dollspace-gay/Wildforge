//! Process command selection. Order is compatibility-sensitive when flags coexist.

mod audits;
mod geography;
mod worlds;
mod entry;
mod qualification;
mod launch;

#[derive(Clone, Copy)]
enum Command {
    MagicQualification,
    AlchemyAudit,
    WorkingsAudit,
    ImplementsAudit,
    DiscoveryAudit,
    MaterialAudit,
    ArcaneAudit,
    ArcaneAtlas,
    ArcaneGeographyAudit,
    ArcaneEcologyAudit,
    ArcaneGeographyExport,
    ArcaneGeographyRetrogen,
    WaterAudit,
    CreateWorld,
    GenerateAtlas,
    ValidateEntry,
    Server,
    ModQualification,
    Agent,
}

impl Command {
    /// Preserve the historical first recognized command, independent of argv order.
    fn select(args: &[String]) -> Option<(Self, usize)> {
        const ORDER: &[(&str, Command)] = &[
            ("--magic-qualification", Command::MagicQualification),
            ("--alchemy-audit", Command::AlchemyAudit),
            ("--workings-audit", Command::WorkingsAudit),
            ("--implements-audit", Command::ImplementsAudit),
            ("--discovery-audit", Command::DiscoveryAudit),
            ("--material-audit", Command::MaterialAudit),
            ("--arcane-audit", Command::ArcaneAudit),
            ("--arcane-atlas", Command::ArcaneAtlas),
            ("--arcane-geography-audit", Command::ArcaneGeographyAudit),
            ("--arcane-ecology-audit", Command::ArcaneEcologyAudit),
            ("--arcane-geography-export", Command::ArcaneGeographyExport),
            ("--arcane-geography-retrogen", Command::ArcaneGeographyRetrogen),
            ("--water-audit", Command::WaterAudit),
            ("--create-world", Command::CreateWorld),
            ("--generate-atlas", Command::GenerateAtlas),
            ("--validate-entry", Command::ValidateEntry),
            ("--server", Command::Server),
            ("--mod-qualification", Command::ModQualification),
            ("--agent", Command::Agent),
        ];
        ORDER.iter().find_map(|(flag, command)| {
            args.iter().position(|arg| arg == flag).map(|i| (*command, i))
        })
    }

    fn execute(self, args: &[String], i: usize) {
        match self {
            Self::MagicQualification => qualification::magic(args, i),
            Self::AlchemyAudit => audits::alchemy(args, i),
            Self::WorkingsAudit => audits::workings(args, i),
            Self::ImplementsAudit => audits::implements(args, i),
            Self::DiscoveryAudit => audits::discovery(args, i),
            Self::MaterialAudit => audits::material(args, i),
            Self::ArcaneAudit => audits::arcane(args, i),
            Self::ArcaneAtlas => geography::dross_atlas(args, i),
            Self::ArcaneGeographyAudit => geography::audit(args, i),
            Self::ArcaneEcologyAudit => geography::ecology(args, i),
            Self::ArcaneGeographyExport => geography::export(args, i),
            Self::ArcaneGeographyRetrogen => geography::retrogen(args, i),
            Self::WaterAudit => audits::water(args, i),
            Self::CreateWorld => worlds::create(args, i),
            Self::GenerateAtlas => worlds::generate_atlas(args, i),
            Self::ValidateEntry => entry::validate(args, i),
            Self::Server => launch::server(args, i),
            Self::ModQualification => qualification::mods(args, i),
            Self::Agent => launch::agent(args, i),
        }
    }
}

pub(crate) fn run() {
    let args: Vec<String> = std::env::args().collect();
    if let Some(result) = crate::geode_capture::run_cli(&args) {
        if let Err(error) = result {
            eprintln!("cracked-geode qualification failed: {error}");
            std::process::exit(1);
        }
        return;
    }
    if let Some((command, index)) = Command::select(&args) {
        command.execute(&args, index);
    } else {
        crate::game::run_windowed();
    }
}
