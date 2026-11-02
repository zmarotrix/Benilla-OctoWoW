//! The start-up data loaders a server's streamed content can run again ([`crate::content_stream`]):
//! each reads its tables off the chain once at start-up, and a pack that changes one of those
//! files while the client runs has it read again, with no restart. Deviation: 1.12.1 reads its
//! tables once per process.
//!
//! A loader is registered beside its start-up registration ([`loader`]) and must only replace
//! resources, so running it twice is the same as running it once. The files a loader reads are
//! learnt the first time it runs here ([`benilla_formats::record_reads`]); until then any change
//! runs it.

use std::collections::HashSet;

use bevy::ecs::system::SystemId;
use bevy::prelude::*;

/// One loader that can run again.
struct Loader {
    name: &'static str,
    id: SystemId,
    /// The chain names it looked up when it last ran here; `None` before it has.
    reads: Option<HashSet<String>>,
}

/// Every loader that can run again, in registration order.
#[derive(Resource, Default)]
pub(crate) struct LiveLoaders(Vec<Loader>);

/// Register `system`, a start-up data loader that only replaces resources, to run again when
/// streamed content changes a file it reads.
pub(crate) fn loader<M>(app: &mut App, system: impl IntoSystem<(), (), M> + 'static) {
    let name = std::any::type_name_of_val(&system);
    let id = app.world_mut().register_system(system);
    app.world_mut()
        .get_resource_or_init::<LiveLoaders>()
        .0
        .push(Loader {
            name,
            id,
            reads: None,
        });
}

/// Run again every loader that read one of `changed` (chain names), or every loader for `None`:
/// the names of the loaders run.
pub(crate) fn rerun(world: &mut World, changed: Option<&HashSet<String>>) -> Vec<&'static str> {
    let Some(mut loaders) = world.remove_resource::<LiveLoaders>() else {
        return Vec::new();
    };
    let mut ran = Vec::new();
    for l in &mut loaders.0 {
        let touched = match (changed, &l.reads) {
            (Some(changed), Some(reads)) => !reads.is_disjoint(changed),
            _ => true,
        };
        if !touched {
            continue;
        }
        let (result, reads) = benilla_formats::record_reads(|| world.run_system(l.id));
        if let Err(e) = result {
            warn!("live data: {} did not run again: {e}", l.name);
            continue;
        }
        l.reads = Some(reads);
        ran.push(l.name);
    }
    world.insert_resource(loaders);
    ran
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Resource, Default)]
    struct Runs(u32);

    fn count(mut runs: ResMut<Runs>) {
        runs.0 += 1;
    }

    /// A loader runs again on any change until its reads are known, then only when one of them
    /// changes; one that looked nothing up never matches a named change again.
    #[test]
    fn a_loader_runs_again_only_when_a_file_it_read_changes() {
        let mut app = App::new();
        app.init_resource::<Runs>();
        loader(&mut app, count);
        let world = app.world_mut();
        let some = HashSet::from(["dbfilesclient\\spell.dbc".to_string()]);
        assert_eq!(rerun(world, Some(&some)).len(), 1, "unknown reads: runs");
        assert_eq!(world.resource::<Runs>().0, 1);
        assert!(
            rerun(world, Some(&some)).is_empty(),
            "read nothing: skipped"
        );
        assert_eq!(rerun(world, None).len(), 1, "everything changed: runs");
        assert_eq!(world.resource::<Runs>().0, 2);
    }
}
