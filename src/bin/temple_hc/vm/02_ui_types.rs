use super::ObjRef;
use super::prelude::*;

#[derive(Clone, Copy, Debug)]
pub(super) struct TempleMsg {
    pub(super) code: i64,
    pub(super) arg1: i64,
    pub(super) arg2: i64,
}

#[derive(Clone, Debug)]
pub(super) enum MenuAction {
    None,
    MsgCmd { arg1: i64, arg2: i64 },
    KeyAscii { ascii: i64 },
    KeyScan { arg2: i64 },
}

#[derive(Clone, Debug)]
pub(super) struct MenuItem {
    pub(super) name: String,
    pub(super) path: String,
    pub(super) entry: ObjRef,
    pub(super) action: MenuAction,
}

#[derive(Clone, Debug)]
pub(super) struct MenuGroup {
    pub(super) name: String,
    pub(super) items: Vec<MenuItem>,
}

#[derive(Clone, Debug)]
pub(super) struct MenuUnderlay {
    pub(super) x: i32,
    pub(super) y: i32,
    pub(super) w: i32,
    pub(super) h: i32,
    pub(super) pixels: Vec<u8>,
}

#[derive(Clone, Debug)]
pub(super) struct MenuState {
    pub(super) root: ObjRef,
    pub(super) groups: Vec<MenuGroup>,
    pub(super) entries_by_path: HashMap<String, ObjRef>,
    pub(super) open_group: Option<usize>,
    pub(super) hover_item: Option<usize>,
    pub(super) underlay: Option<MenuUnderlay>,
}
