//! Exercise the owner without creating a window or borrowing a running server.

use super::*;
use std::sync::mpsc::channel;
use std::time::{Duration, Instant};

const TIMEOUT: Duration = Duration::from_secs(10);

fn registry() -> Arc<Registry> {
    Arc::new(crate::registry::load(std::path::Path::new(
        "/nonexistent-mods-dir",
    )))
}

fn terminal(loading: &mut WorldLoading, reg: &Arc<Registry>) -> LoadingEvent {
    let deadline = Instant::now() + TIMEOUT;
    loop {
        match loading.poll(reg) {
            None | Some(LoadingEvent::Progress(_)) => {}
            Some(event) => return event,
        }
        assert!(Instant::now() < deadline, "loading did not terminate");
        std::thread::yield_now();
    }
}

fn prepared(reg: Arc<Registry>) -> WorkOutput {
    let world = World::new(42, PathBuf::from("saves/.none"), reg);
    let spawn = EntityPos::new(crate::planet::Face::PosZ, 500.5, 80.0, 500.5).unwrap();
    WorkOutput::Ready(Box::new(PreparedWorld {
        world,
        spawn,
        notices: vec!["test notice".into()],
    }))
}

#[test]
fn one_operation_owns_the_slot_until_cancelled_worker_has_joined() {
    let reg = registry();
    let mut loading = WorldLoading::default();
    let (started, running) = channel();
    let (release, resume) = channel();
    loading
        .begin(
            "first".into(),
            LoadingKind::Creation,
            Arc::clone(&reg),
            move |_, cancel| {
                started.send(()).unwrap();
                resume.recv_timeout(TIMEOUT).unwrap();
                assert!(cancel.is_cancelled());
                Err(io::Error::new(io::ErrorKind::Interrupted, "cancelled"))
            },
        )
        .unwrap();
    running.recv_timeout(TIMEOUT).unwrap();
    assert_eq!(
        loading
            .begin(
                "second".into(),
                LoadingKind::Entry {
                    created_here: false
                },
                Arc::clone(&reg),
                |_, _| panic!("busy operation must not start")
            )
            .unwrap_err()
            .kind(),
        io::ErrorKind::WouldBlock
    );
    assert_eq!(loading.cancel(), Some(LoadingKind::Creation));
    assert!(loading.is_active());
    release.send(()).unwrap();
    assert!(matches!(
        terminal(&mut loading, &reg),
        LoadingEvent::Cancelled(LoadingKind::Creation)
    ));
    assert!(!loading.is_active());
    assert!(loading.poll(&reg).is_none());
    loading
        .begin(
            "next".into(),
            LoadingKind::Creation,
            Arc::clone(&reg),
            |_, _| Ok(WorkOutput::Created),
        )
        .unwrap();
    assert!(matches!(
        terminal(&mut loading, &reg),
        LoadingEvent::Created { enter: true, .. }
    ));
}

#[test]
fn cancelled_successful_entry_is_discarded_and_content_replacement_restarts_it() {
    let reg = registry();
    let changed_reg = registry();
    for cancelled in [false, true] {
        let mut loading = WorldLoading::default();
        let work_reg = Arc::clone(&reg);
        let (ready, prepared_rx) = channel();
        let (release, resume) = channel();
        loading
            .begin(
                "saved".into(),
                LoadingKind::Entry { created_here: true },
                Arc::clone(&reg),
                move |_, _| {
                    let result = prepared(work_reg);
                    ready.send(()).unwrap();
                    resume.recv_timeout(TIMEOUT).unwrap();
                    Ok(result)
                },
            )
            .unwrap();
        prepared_rx.recv_timeout(TIMEOUT).unwrap();
        if cancelled {
            loading.cancel();
        }
        release.send(()).unwrap();
        let event = terminal(&mut loading, &changed_reg);
        if cancelled {
            assert!(matches!(
                event,
                LoadingEvent::Cancelled(LoadingKind::Entry { created_here: true })
            ));
        } else {
            assert!(
                matches!(event, LoadingEvent::Reenter { name, created_here: true } if name == "saved")
            );
        }
        assert!(!loading.is_active());
    }
}

#[test]
fn a_private_placeholder_registry_does_not_invalidate_the_requested_content() {
    let reg = registry();
    let private_reg = Arc::new((*reg).clone());
    let mut loading = WorldLoading::default();
    loading
        .begin(
            "saved".into(),
            LoadingKind::Entry {
                created_here: false,
            },
            Arc::clone(&reg),
            move |_, _| Ok(prepared(private_reg)),
        )
        .unwrap();
    let LoadingEvent::Ready { name, prepared } = terminal(&mut loading, &reg) else {
        panic!("requested content did not change");
    };
    assert_eq!(name, "saved");
    assert_eq!(prepared.notices, ["test notice"]);
    assert!(!Arc::ptr_eq(&prepared.world.reg, &reg));
}

#[test]
fn cancellation_does_not_hide_read_failures_or_panics() {
    let reg = registry();
    for panic_worker in [false, true] {
        let mut loading = WorldLoading::default();
        loading
            .begin(
                "broken".into(),
                LoadingKind::Entry {
                    created_here: false,
                },
                Arc::clone(&reg),
                move |_, _| {
                    if panic_worker {
                        panic!("fixture loader panic");
                    }
                    Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        "cancel is part of this corrupt filename",
                    ))
                },
            )
            .unwrap();
        loading.cancel();
        let LoadingEvent::Failed { error, .. } = terminal(&mut loading, &reg) else {
            panic!("real failure must not be classified by cancellation or text");
        };
        if panic_worker {
            assert!(error.to_string().contains("fixture loader panic"));
        } else {
            assert_eq!(error.kind(), io::ErrorKind::InvalidData);
        }
    }
}

#[test]
fn published_world_survives_cancellation_and_can_be_listed_again() {
    let root = std::env::temp_dir().join(format!(
        "wildforge-loading-published-{}",
        std::process::id()
    ));
    let destination = root.join("planet");
    assert!(!root.exists(), "fixture needs a fresh destination");
    let reg = registry();
    let mut loading = WorldLoading::default();
    let (published, ready) = channel();
    let (release, resume) = channel();
    loading
        .begin(
            "planet".into(),
            LoadingKind::Creation,
            Arc::clone(&reg),
            move |_, cancel| {
                crate::world::create_world_fixture_atomic(
                    &destination,
                    42,
                    "survival",
                    8,
                    &cancel,
                    |_| {},
                )?;
                published.send(()).unwrap();
                resume.recv_timeout(TIMEOUT).unwrap();
                Ok(WorkOutput::Created)
            },
        )
        .unwrap();
    ready.recv_timeout(TIMEOUT).unwrap();
    loading.cancel();
    release.send(()).unwrap();
    assert!(matches!(
        terminal(&mut loading, &reg),
        LoadingEvent::Created { enter: false, .. }
    ));
    assert_eq!(
        crate::world::list_worlds(&root),
        [("planet".to_string(), 42)]
    );
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn atlas_and_arcane_cancellation_remain_typed() {
    assert!(is_cancellation(&io::Error::other(
        crate::planet_atlas::AtlasError::Cancelled
    )));
    assert!(is_cancellation(&io::Error::other(
        crate::arcane_geography::ArcaneGeographyError::Cancelled
    )));
    assert!(!is_cancellation(&io::Error::other(
        "planet creation cancelled"
    )));
}
