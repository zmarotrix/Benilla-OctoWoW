//! nampower's reads, which OctoWoW's client mods add for pfUI, Cursive and HealersMate: a unit's
//! raw update fields, a `Spell.dbc` row, a spell icon's path, a spell's usable and range tests
//! ([`benilla_ui::script`]'s nampower queries), answered off the objects and the spell catalog as of the call. Deviation: 1.12.1 has
//! no such reads.

use bevy::ecs::system::SystemParam;
use bevy::prelude::*;

use benilla_ui::script::live::LiveAsk;
use benilla_ui::script::{
    CastTimeQuery, SpellIconQuery, SpellNowQuery, SpellQueueQuery, SpellRangeQuery, SpellRowAnswer,
    SpellRowQuery, UnitFieldsQuery,
};

use crate::net::GuidIndex;
use crate::ui_action::Spells;

/// The update fields a unit read covers: the object and unit blocks, `UNIT_END` 188.
const UNIT_END: usize = 188;

/// What nampower's reads look at.
#[derive(SystemParam)]
pub(crate) struct NampowerLive<'w, 's> {
    index: Option<Res<'w, GuidIndex>>,
    stores: crate::ui_unit::views::ViewStores<'w, 's>,
    spells: Option<Res<'w, Spells>>,
    /// The action bar's slot walk, whose usable and range tests answer for any spell.
    walk: crate::ui_action::ActionWalk<'w, 's>,
    /// The client's spell queue, which the queue verbs read and clear.
    queue: Option<Res<'w, crate::spell::queue::SpellQueue>>,
}

impl NampowerLive<'_, '_> {
    /// The unit's fields `0..UNIT_END` by index, 0 where unsent; `None` with no object held.
    fn unit_fields(&self, guid: u64) -> Option<Vec<u32>> {
        let entity = self.index.as_deref().and_then(|i| i.0.get(&guid).copied());
        let store = self.stores.at(guid, entity)?;
        if !store.is_unit() {
            return None;
        }
        let mut words = vec![0u32; UNIT_END];
        for (i, v) in store.0.raw_fields() {
            if let Some(w) = words.get_mut(usize::from(i)) {
                *w = v;
            }
        }
        Some(words)
    }

    fn spell_row(&self, id: u32) -> Option<SpellRowAnswer> {
        let row = self.spells.as_deref()?.catalog.row(id)?;
        Some(SpellRowAnswer {
            columns: row.columns.to_vec(),
            name: row.name.clone(),
            rank: row.rank.clone(),
            description: row.description.clone(),
            tooltip: row.tooltip.clone(),
        })
    }
}

impl crate::script_calls::live::LiveFamily for NampowerLive<'_, '_> {
    fn answer(&mut self, ask: &mut LiveAsk<'_>) -> bool {
        ask.answer(|q: &UnitFieldsQuery| Some(self.unit_fields(q.0)))
            || ask.answer(|q: &SpellRowQuery| {
                self.spells.as_ref()?;
                Some(self.spell_row(q.0))
            })
            || ask.answer(|q: &SpellNowQuery| Some(self.walk.spell_now(q.spell, q.unit)))
            || ask.answer(|q: &SpellQueueQuery| {
                let queue = self.queue.as_deref()?;
                match *q {
                    SpellQueueQuery::Peek => {}
                    SpellQueueQuery::Clear => queue.clear(),
                    SpellQueueQuery::SkipNext(on) => queue.skip_next(on),
                }
                Some(queue.peek().unwrap_or(0))
            })
            || ask.answer(|q: &SpellRangeQuery| {
                let spells = self.spells.as_deref()?;
                Some(spells.ranges.get(q.0).map(|r| (r.min, r.max, r.flags)))
            })
            || ask.answer(|q: &CastTimeQuery| {
                let spells = self.spells.as_deref()?;
                Some(spells.catalog.get(q.0).map(|d| {
                    spells
                        .cast_times
                        .get(d.casting_time_index)
                        .map_or(0, |row| row.base_ms.max(0) as u32)
                }))
            })
            || ask.answer(|q: &SpellIconQuery| {
                let spells = self.spells.as_deref()?;
                Some(spells.catalog.icon_path(q.0).map(str::to_owned))
            })
    }
}
