//! [`World`] on the running Elden Ring (fromsoftware-rs bindings, hooks.json).

use crate::game::{EnemyState, PlayerState, World};
use eldenring::cs::{ChrIns, FieldInsHandle, WorldChrMan, WorldChrManDbgFlags};
use fromsoftware_shared::FromStatic;
use std::collections::HashMap;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::ptr::NonNull;

pub struct ErWorld {
    /// This frame's characters by key, for writes after `enemies()`.
    seen: HashMap<u64, NonNull<ChrIns>>,
}

fn key(h: &FieldInsHandle) -> u64 {
    let mut s = DefaultHasher::new();
    h.hash(&mut s);
    s.finish()
}

fn pos(c: &ChrIns) -> [f32; 3] {
    let p = &c.modules.physics.position;
    [p.0, p.1, p.2]
}

// Only touched from the game's own task thread, within one frame.
unsafe impl Send for ErWorld {}

impl ErWorld {
    pub fn new() -> Self {
        Self { seen: HashMap::new() }
    }

    fn wcm() -> Option<&'static mut WorldChrMan> {
        unsafe { WorldChrMan::instance_mut() }.ok()
    }
}

impl World for ErWorld {
    fn player(&mut self) -> Option<PlayerState> {
        let wcm = Self::wcm()?;
        let p = wcm.main_player.as_ref()?;
        let d = &p.chr_ins.modules.data;
        if d.max_hp <= 0 {
            return None;
        }
        Some(PlayerState { hp: d.hp, max_hp: d.max_hp, stamina: d.stamina, max_stamina: d.max_stamina, pos: pos(&p.chr_ins) })
    }

    fn set_player_hp(&mut self, hp: i32) {
        if let Some(p) = Self::wcm().and_then(|w| w.main_player.as_mut()) {
            p.chr_ins.modules.data.hp = hp;
        }
    }

    fn set_player_stamina(&mut self, stamina: i32) {
        if let Some(p) = Self::wcm().and_then(|w| w.main_player.as_mut()) {
            p.chr_ins.modules.data.stamina = stamina;
        }
    }

    fn set_player_no_dead(&mut self, on: bool) {
        match unsafe { WorldChrManDbgFlags::instance_mut() } {
            Ok(f) => f.player_no_dead = on,
            Err(e) => crate::plugin::log(format!("no-death flag unavailable: {e:?}")),
        }
    }

    fn enemies(&mut self) -> Vec<EnemyState> {
        self.seen.clear();
        let Some(wcm) = Self::wcm() else { return Vec::new() };
        let Some(player) = wcm.main_player.as_ref() else { return Vec::new() };
        let me = player.chr_ins.field_ins_handle;
        let mut out = Vec::new();
        for set in wcm.chr_sets.iter().flatten() {
            for c in set.characters() {
                let h = c.field_ins_handle;
                if h == me || h.is_empty() {
                    continue;
                }
                let k = key(&h);
                if self.seen.contains_key(&k) {
                    continue;
                }
                let d = &c.modules.data;
                if d.max_hp <= 0 {
                    continue;
                }
                out.push(EnemyState { key: k, hp: d.hp, max_hp: d.max_hp, last_hit_by_player: c.last_hit_by == me, pos: pos(c) });
                self.seen.insert(k, NonNull::from(&mut *c));
            }
        }
        out
    }

    fn set_enemy_hp(&mut self, key: u64, hp: i32, max_hp: Option<i32>) {
        if let Some(c) = self.seen.get_mut(&key) {
            let c = unsafe { c.as_mut() };
            let d = &mut c.modules.data;
            if let Some(m) = max_hp {
                d.max_hp = m;
            }
            d.hp = hp;
        }
    }
}
