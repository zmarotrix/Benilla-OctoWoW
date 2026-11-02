//! The dressing room's look, the reference's `DressUpFrame`/`DressUpModel`: the player's own
//! character wearing items they do not own. Nobody in the world wears the preview, so it is built
//! from a spec by the shared assembly ([`crate::entities::attach`]), not mirrored from a live
//! entity, and it dresses by the select-screen law (weapons in hand, `0x47a0c0`). A `DressUpModel`
//! pane on the player draws this assembly ([`super::panes`]); `DressUpModel` (`0x495c00`) shares
//! the `CharacterModelBase` ctor (`0x505680`) with every character pane.
//!
//! Each `DressUpModel` pane wears its own try-ons and draws its own assembly, as the reference's
//! widgets each clone their own model: the dressing room, the auction house's, and a server's
//! own panes (OctoWoW's shop cards, several at once).

use std::collections::HashMap;

use benilla_protocol::CharEnumItem;
use benilla_ui::widget::FrameHandle;
use bevy::prelude::*;

use super::{PreviewBillboard, PreviewEffects, PreviewPart, PreviewRider};

/// The player's own body and appearance wearing [`crate::ui_dressup`]'s equipment array (their
/// visible items with the tried-on ones substituted); a change in it re-assembles the booth.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) struct DressUpLook {
    /// The player's own body display id (the reference's `SetUnit("player")`).
    pub(crate) display_id: u32,
    pub(crate) race: u8,
    pub(crate) sex: u8,
    pub(crate) skin: u8,
    pub(crate) face: u8,
    pub(crate) hair_style: u8,
    pub(crate) hair_color: u8,
    pub(crate) facial_hair: u8,
    /// `ItemDisplayInfo` ids by equipment slot, in the `SMSG_CHAR_ENUM` shape (helm 0, main hand
    /// 15, off hand 16, ranged 17, tabard 18).
    pub(crate) equipment: [CharEnumItem; 19],
    /// The player's own guild crest, so a tried-on Guild Tabard shows it, not the blank default.
    pub(crate) emblem: Option<benilla_formats::GuildEmblem>,
}

/// The dressing rooms' input: each shown pane's look, by pane.
#[derive(Resource, Default)]
pub(crate) struct DressUpPreview {
    pub(crate) looks: HashMap<FrameHandle, DressUpLook>,
}

/// Each pane's assembled dressing-room parts, by pane.
#[derive(Resource, Default)]
pub(crate) struct DressUpBakes(pub(crate) HashMap<FrameHandle, DressUpBake>);

/// One pane's assembled dressing-room parts. `revision` bumps on a fresh assembly or a clear; the
/// pane drawing it re-dresses only when it moves.
#[derive(Default)]
pub(crate) struct DressUpBake {
    pub(crate) display_id: u32,
    pub(crate) parts: Vec<PreviewPart>,
    pub(crate) riders: Vec<PreviewRider>,
    pub(crate) effects: Vec<PreviewEffects>,
    pub(crate) billboards: Vec<PreviewBillboard>,
    pub(crate) grip: [bool; 2],
    pub(crate) revision: u64,
}
