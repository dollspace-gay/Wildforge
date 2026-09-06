//! Scoped script reads restore the prior owner after nesting and unwinding.

use super::{WorldGuard, with_world};
use crate::world::ReplicaWorld;

fn replica(seed: u32) -> ReplicaWorld {
    ReplicaWorld::new(
        seed,
        std::sync::Arc::new(crate::registry::empty_for_test()),
        0.0,
    )
}

fn current_seed() -> Option<u32> {
    with_world(|view| Some(view.seed()), None)
}

#[test]
fn nested_dispatch_restores_each_borrowed_view() {
    let first = replica(17);
    let second = replica(29);
    assert_eq!(current_seed(), None);
    {
        let view = first.view();
        let _outer = WorldGuard::new(&view);
        assert_eq!(current_seed(), Some(17));
        {
            let view = second.view();
            let _inner = WorldGuard::new(&view);
            assert_eq!(current_seed(), Some(29));
        }
        assert_eq!(current_seed(), Some(17));
    }
    assert_eq!(current_seed(), None);
}

#[test]
fn unwinding_inner_dispatch_restores_outer_then_clears_the_context() {
    let first = replica(17);
    let second = replica(29);
    let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let view = first.view();
        let _outer = WorldGuard::new(&view);
        let inner = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let view = second.view();
            let _inner = WorldGuard::new(&view);
            assert_eq!(current_seed(), Some(29));
            panic!("nested fixture unwind");
        }));
        assert!(inner.is_err());
        assert_eq!(current_seed(), Some(17));
        panic!("outer fixture unwind");
    }));
    assert!(outcome.is_err());
    assert_eq!(current_seed(), None);
    drop((first, second));
    assert_eq!(current_seed(), None);
}

#[test]
fn scoped_context_never_crosses_threads() {
    let world = replica(17);
    let view = world.view();
    let _guard = WorldGuard::new(&view);
    assert_eq!(std::thread::spawn(current_seed).join().unwrap(), None);
    assert_eq!(current_seed(), Some(17));
}
