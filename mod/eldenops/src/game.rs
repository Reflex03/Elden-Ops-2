//! Black Ops II Zombies rules running in the Lands Between.
//!
//! Pure logic: it sees Elden Ring only through [`World`] and plays sounds through [`Sfx`], so the
//! whole round flow is unit-tested without the game (see the tests at the bottom).

use crate::sheets::*;
use std::collections::HashMap;

#[derive(Clone, Copy, Debug, Default)]
pub struct PlayerState {
    pub hp: i32,
    pub max_hp: i32,
    pub stamina: i32,
    pub max_stamina: i32,
    pub pos: [f32; 3],
}

#[derive(Clone, Copy, Debug)]
pub struct EnemyState {
    pub key: u64,
    pub hp: i32,
    pub max_hp: i32,
    /// `ChrIns.last_hit_by` is the main player.
    pub last_hit_by_player: bool,
    pub pos: [f32; 3],
}

/// Elden Ring, as the round logic needs it (hooks.json).
pub trait World {
    fn player(&mut self) -> Option<PlayerState>;
    fn set_player_hp(&mut self, hp: i32);
    fn set_player_stamina(&mut self, stamina: i32);
    fn set_player_no_dead(&mut self, on: bool);
    fn enemies(&mut self) -> Vec<EnemyState>;
    fn set_enemy_hp(&mut self, key: u64, hp: i32, max_hp: Option<i32>);
}

pub trait Sfx {
    fn play(&self, snd: Snd);
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    WaitingForWorld,
    PreRound,
    InRound,
    BetweenRounds,
    Dead,
}

#[derive(Clone, Copy, Debug)]
struct Track {
    last_hp: i32,
    scaled_round: i32,
}

#[derive(Clone, Debug, Default)]
pub struct HudSnapshot {
    pub visible: bool,
    pub round: i32,
    pub points: i32,
    pub owned: Vec<&'static str>,
    pub message: Option<String>,
    pub shop: Vec<(&'static str, &'static str, i32, bool)>,
    pub jugg_pool: i32,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Stats {
    pub hits: u32,
    pub kills: u32,
    pub rounds_started: u32,
    pub perks_bought: u32,
    pub revives: u32,
    pub game_overs: u32,
}

pub struct Game {
    pub phase: Phase,
    pub round: i32,
    pub points: i32,
    pub owned: [bool; 4],
    pub kills: i32,
    pub needed: i32,
    timer: f32,
    jingle_at: Option<f32>,
    alive_for: f32,
    enemies: HashMap<u64, Track>,
    orig_max: HashMap<u64, i32>,
    player_last_hp: i32,
    player_last_max: i32,
    stamina_acc: f32,
    no_dead_on: bool,
    no_dead_on_last: bool,
    jugg_pool: i32,
    player_max_seen: i32,
    keys_were: [bool; 4],
    message: Option<(String, f32)>,
    queued: Vec<(f32, Snd)>,
    rng: u32,
    pub stats: Stats,
    pub log: Vec<String>,
}

impl Default for Game {
    fn default() -> Self {
        Self::new(0x9E37_79B9)
    }
}

/// BO2's `ai_calculate_health` (_zm.gsc).
pub fn zombie_health(round: i32) -> i32 {
    let mut h = ZOMBIE_HEALTH_START;
    for i in 2..=round {
        if i >= 10 {
            let old = h;
            h += (h as f32 * ZOMBIE_HEALTH_INCREASE_MULTIPLIER) as i32;
            if h < old {
                return old;
            }
        } else {
            h += ZOMBIE_HEALTH_INCREASE;
        }
    }
    h
}

/// BO2's solo zombie total for a round (_zm.gsc round_spawning + default_max_zombie_func).
pub fn zombie_total(round: i32) -> i32 {
    let mut multiplier = round as f32 / 5.0;
    if multiplier < 1.0 {
        multiplier = 1.0;
    }
    if round >= 10 {
        multiplier *= round as f32 * 0.15;
    }
    let max = ZOMBIE_MAX_AI + (0.5 * ZOMBIE_AI_PER_PLAYER as f32 * multiplier) as i32;
    let f = match round {
        r if r < 2 => 0.25,
        r if r < 3 => 0.3,
        r if r < 4 => 0.5,
        r if r < 5 => 0.7,
        r if r < 6 => 0.9,
        _ => return max,
    };
    (max as f32 * f) as i32
}

/// How much tougher Elden Ring's enemies get this round (round_rules: er_health_scale).
pub fn health_multiplier(round: i32) -> f32 {
    1.0 + ER_HEALTH_SCALE * (zombie_health(round) as f32 / ZOMBIE_HEALTH_START as f32 - 1.0)
}

fn dist(a: [f32; 3], b: [f32; 3]) -> f32 {
    ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)).sqrt()
}

impl Game {
    pub fn new(seed: u32) -> Self {
        Self {
            phase: Phase::WaitingForWorld,
            round: 0,
            points: STARTING_POINTS,
            owned: [false; 4],
            kills: 0,
            needed: 0,
            timer: 0.0,
            jingle_at: None,
            alive_for: 0.0,
            enemies: HashMap::new(),
            orig_max: HashMap::new(),
            player_last_hp: 0,
            player_last_max: 0,
            stamina_acc: 0.0,
            no_dead_on: false,
            no_dead_on_last: false,
            jugg_pool: 0,
            player_max_seen: 0,
            keys_were: [false; 4],
            message: None,
            queued: Vec::new(),
            rng: seed | 1,
            stats: Stats::default(),
            log: Vec::new(),
        }
    }

    fn owns(&self, effect: PerkEffect) -> Option<&'static PerkRow> {
        PERKS.iter().zip(self.owned).find(|(p, o)| *o && p.effect == effect).map(|(p, _)| p)
    }

    fn say(&mut self, text: String, secs: f32) {
        self.message = Some((text, secs));
    }

    fn note(&mut self, m: String) {
        self.log.push(m);
    }

    fn next_rand(&mut self) -> u32 {
        self.rng ^= self.rng << 13;
        self.rng ^= self.rng >> 17;
        self.rng ^= self.rng << 5;
        self.rng
    }

    fn start_round(&mut self, round: i32, sfx: &dyn Sfx) {
        self.round = round;
        self.kills = 0;
        self.needed = zombie_total(round);
        self.phase = Phase::InRound;
        self.stats.rounds_started += 1;
        self.refill_jugg();
        sfx.play(Snd::RoundStart);
        self.say(format!("ROUND {round}"), 4.0);
        self.note(format!("round {round} started: {} kills, enemy hp x{:.2}", self.needed, health_multiplier(round)));
    }

    fn reset_run(&mut self) {
        self.round = 0;
        self.points = STARTING_POINTS;
        self.owned = [false; 4];
        self.kills = 0;
        self.needed = 0;
        self.orig_max.clear();
        self.phase = Phase::PreRound;
        self.timer = FIRST_ROUND_DELAY as f32;
    }

    /// One game frame. `keys` are the perk keys (perks.json key_vk) held this frame.
    pub fn tick(&mut self, dt: f32, world: &mut dyn World, sfx: &dyn Sfx, keys: [bool; 4]) {
        let dt = dt.clamp(0.0, 0.25);
        self.queued.retain_mut(|(t, s)| {
            *t -= dt;
            if *t <= 0.0 {
                sfx.play(*s);
                false
            } else {
                true
            }
        });
        if let Some((_, t)) = &mut self.message {
            *t -= dt;
            if *t <= 0.0 {
                self.message = None;
            }
        }

        let Some(mut player) = world.player() else {
            // Loading screen or title: hold the run, forget who was loaded.
            self.enemies.clear();
            self.keys_were = keys;
            return;
        };
        if self.phase == Phase::WaitingForWorld {
            self.reset_run();
            self.player_last_hp = player.hp;
            self.player_last_max = player.max_hp;
            self.note("loaded into the Lands Between".into());
        }

        self.player_max_seen = player.max_hp;
        self.player_tick(dt, world, sfx, &mut player);
        if self.phase != Phase::Dead {
            self.enemy_tick(world, &player);
            self.round_tick(dt, sfx);
            self.shop_tick(sfx, keys, &player);
        }
        self.keys_were = keys;
    }

    fn player_tick(&mut self, dt: f32, world: &mut dyn World, sfx: &dyn Sfx, p: &mut PlayerState) {
        if self.phase == Phase::Dead {
            if p.hp > 0 {
                self.alive_for += dt;
                if self.alive_for > 2.0 {
                    self.note("respawned: new game from round 1".into());
                    self.reset_run();
                }
            }
            self.player_last_hp = p.hp;
            self.player_last_max = p.max_hp;
            return;
        }

        // Juggernog: BO2 health 100 -> 250, as a pool of extra HP that absorbs hits.
        // Quick Revive: stand the player back up once. Both need the game to stop a lethal hit at
        // 1 HP (no-death flag) so the hit can still be absorbed or revived.
        let has_jugg = self.owns(PerkEffect::BonusHealthPoolOfMax).is_some();
        let has_revive = self.owns(PerkEffect::SelfReviveFullHpOnce).is_some();
        if has_jugg && p.hp > 0 && p.hp < self.player_last_hp && p.max_hp == self.player_last_max && self.jugg_pool > 0 {
            let refund = (self.player_last_hp - p.hp).min(self.jugg_pool);
            self.jugg_pool -= refund;
            p.hp = (p.hp + refund).min(p.max_hp);
            world.set_player_hp(p.hp);
        }
        let arm = has_revive || (has_jugg && self.jugg_pool > 0);
        if arm != self.no_dead_on {
            world.set_player_no_dead(arm);
            self.no_dead_on = arm;
        }
        if p.hp <= 1 && p.max_hp > 1 && self.player_last_hp > 1 && !arm && self.no_dead_on_last {
            // The Juggernog pool ran dry on this hit: it lands at 1 HP, the next one kills.
            self.say("JUGGERNOG DRAINED".into(), 2.0);
        }
        if has_revive && p.hp <= 1 && p.max_hp > 1 {
            p.hp = p.max_hp;
            world.set_player_hp(p.hp);
            self.owned = [false; 4]; // downed in BO2 = perks lost
            self.jugg_pool = 0;
            world.set_player_no_dead(false);
            self.no_dead_on = false;
            self.stats.revives += 1;
            sfx.play(Snd::ReviveSting);
            self.say("QUICK REVIVE".into(), 3.0);
            self.note("quick revive used".into());
        }
        self.no_dead_on_last = self.no_dead_on;

        // Speed Cola: faster stamina recovery.
        if let Some(speed) = self.owns(PerkEffect::StaminaRegenPerSecondOfMax) {
            if p.stamina < p.max_stamina {
                self.stamina_acc += p.max_stamina as f32 * speed.effect_value * dt;
                let add = self.stamina_acc.floor() as i32;
                if add > 0 {
                    self.stamina_acc -= add as f32;
                    p.stamina = (p.stamina + add).min(p.max_stamina);
                    world.set_player_stamina(p.stamina);
                }
            } else {
                self.stamina_acc = 0.0;
            }
        }

        if p.hp <= 0 {
            self.stats.game_overs += 1;
            let rounds = self.round.max(1);
            self.phase = Phase::Dead;
            self.alive_for = 0.0;
            self.owned = [false; 4];
            self.jugg_pool = 0;
            self.queued.clear();
            if self.no_dead_on {
                world.set_player_no_dead(false);
                self.no_dead_on = false;
            }
            sfx.play(Snd::GameOver);
            let s = if rounds == 1 { "" } else { "S" };
            self.say(format!("GAME OVER\nYOU SURVIVED {rounds} ROUND{s}"), 9.0);
            self.note(format!("game over after round {rounds}"));
        }
        self.player_last_hp = p.hp;
        self.player_last_max = p.max_hp;
    }

    fn enemy_tick(&mut self, world: &mut dyn World, p: &PlayerState) {
        let enemies = world.enemies();
        let double_tap = self.owns(PerkEffect::DamageDealtMultiplier).map(|d| d.effect_value);
        let mut seen = Vec::with_capacity(enemies.len());
        for e in enemies {
            seen.push(e.key);
            let orig = *self.orig_max.entry(e.key).or_insert(e.max_hp);
            let track = self.enemies.entry(e.key).or_insert(Track { last_hp: e.hp, scaled_round: 0 });

            // Round health: an untouched enemy near the player joins this round's horde.
            if self.phase == Phase::InRound
                && e.hp > 0
                && e.hp == e.max_hp
                && track.scaled_round != self.round
                && dist(e.pos, p.pos) <= HORDE_RADIUS
            {
                let max = ((orig as f32) * health_multiplier(self.round)).round().max(1.0) as i32;
                world.set_enemy_hp(e.key, max, Some(max));
                track.scaled_round = self.round;
                track.last_hp = max;
                continue;
            }

            if e.hp < track.last_hp && e.last_hit_by_player {
                let mut hp = e.hp;
                self.points += SCORE_DAMAGE;
                self.stats.hits += 1;
                if let (Some(mult), true) = (double_tap, hp > 0) {
                    let extra = ((track.last_hp - hp) as f32 * (mult - 1.0)).round() as i32;
                    // Leave the killing blow to the game's own hit.
                    let new_hp = (hp - extra).max(1);
                    if new_hp < hp {
                        hp = new_hp;
                        world.set_enemy_hp(e.key, hp, None);
                    }
                }
                if hp <= 0 {
                    self.points += SCORE_KILL;
                    self.stats.kills += 1;
                    if self.phase == Phase::InRound {
                        self.kills += 1;
                    }
                }
                track.last_hp = hp;
            } else {
                track.last_hp = e.hp;
            }
        }
        self.enemies.retain(|k, _| seen.contains(k));
        if self.orig_max.len() > 4096 {
            self.orig_max.retain(|k, _| seen.contains(k));
        }
    }

    fn round_tick(&mut self, dt: f32, sfx: &dyn Sfx) {
        match self.phase {
            Phase::PreRound => {
                self.timer -= dt;
                if self.timer <= 0.0 {
                    self.start_round(self.round + 1, sfx);
                }
            }
            Phase::InRound => {
                if self.kills >= self.needed {
                    sfx.play(Snd::RoundOver);
                    self.phase = Phase::BetweenRounds;
                    self.timer = ZOMBIE_BETWEEN_ROUND_TIME as f32;
                    self.jingle_at = Some(ZOMBIE_BETWEEN_ROUND_TIME as f32 * 0.6);
                    self.note(format!("round {} over", self.round));
                }
            }
            Phase::BetweenRounds => {
                self.timer -= dt;
                if let Some(at) = self.jingle_at {
                    if self.timer <= at {
                        self.jingle_at = None;
                        let unowned: Vec<usize> = (0..PERKS.len()).filter(|i| !self.owned[*i]).collect();
                        if !unowned.is_empty() {
                            let i = unowned[self.next_rand() as usize % unowned.len()];
                            sfx.play(PERKS[i].jingle);
                        }
                    }
                }
                if self.timer <= 0.0 {
                    self.start_round(self.round + 1, sfx);
                }
            }
            Phase::WaitingForWorld | Phase::Dead => {}
        }
    }

    fn shop_tick(&mut self, sfx: &dyn Sfx, keys: [bool; 4], p: &PlayerState) {
        if p.hp <= 0 {
            return;
        }
        for i in 0..PERKS.len() {
            if keys[i] && !self.keys_were[i] {
                self.buy(i, sfx);
            }
        }
    }

    fn refill_jugg(&mut self) {
        if let Some(j) = self.owns(PerkEffect::BonusHealthPoolOfMax) {
            self.jugg_pool = (self.player_max_seen as f32 * j.effect_value).round() as i32;
        }
    }

    pub fn buy(&mut self, i: usize, sfx: &dyn Sfx) {
        let perk = &PERKS[i];
        if self.owned[i] {
            sfx.play(Snd::Deny);
            return;
        }
        if self.points < perk.cost {
            sfx.play(Snd::Deny);
            self.say("NOT ENOUGH POINTS".into(), 2.0);
            return;
        }
        self.points -= perk.cost;
        self.owned[i] = true;
        if perk.effect == PerkEffect::BonusHealthPoolOfMax {
            self.refill_jugg();
        }
        self.stats.perks_bought += 1;
        sfx.play(Snd::Purchase);
        self.queued.push((0.35, perk.sting));
        self.queued.push((2.2, Snd::Bump));
        self.say(perk.name.to_uppercase(), 3.0);
        self.note(format!("bought {} for {}", perk.name, perk.cost));
    }

    pub fn hud(&self) -> HudSnapshot {
        HudSnapshot {
            visible: self.phase != Phase::WaitingForWorld,
            round: self.round,
            points: self.points,
            owned: PERKS.iter().zip(self.owned).filter(|(_, o)| *o).map(|(p, _)| p.name).collect(),
            message: self.message.as_ref().map(|(m, _)| m.clone()),
            shop: PERKS.iter().zip(self.owned).map(|(p, o)| (p.key, p.name, p.cost, o)).collect(),
            jugg_pool: self.jugg_pool,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;

    #[derive(Default)]
    struct FakeWorld {
        player: Option<PlayerState>,
        no_dead: bool,
        enemies: Vec<EnemyState>,
    }
    impl World for FakeWorld {
        fn player(&mut self) -> Option<PlayerState> {
            self.player
        }
        fn set_player_hp(&mut self, hp: i32) {
            self.player.as_mut().unwrap().hp = hp;
        }
        fn set_player_stamina(&mut self, s: i32) {
            self.player.as_mut().unwrap().stamina = s;
        }
        fn set_player_no_dead(&mut self, on: bool) {
            self.no_dead = on;
        }
        fn enemies(&mut self) -> Vec<EnemyState> {
            self.enemies.clone()
        }
        fn set_enemy_hp(&mut self, key: u64, hp: i32, max: Option<i32>) {
            let e = self.enemies.iter_mut().find(|e| e.key == key).unwrap();
            e.hp = hp;
            if let Some(m) = max {
                e.max_hp = m;
            }
        }
    }
    #[derive(Default)]
    struct Rec(RefCell<Vec<Snd>>);
    impl Sfx for Rec {
        fn play(&self, s: Snd) {
            self.0.borrow_mut().push(s);
        }
    }

    fn player() -> PlayerState {
        PlayerState { hp: 1000, max_hp: 1000, stamina: 100, max_stamina: 100, pos: [0.0; 3] }
    }
    fn enemy(key: u64, hp: i32) -> EnemyState {
        EnemyState { key, hp, max_hp: hp, last_hit_by_player: false, pos: [5.0, 0.0, 0.0] }
    }
    fn run(g: &mut Game, w: &mut FakeWorld, s: &Rec, secs: f32) {
        for _ in 0..(secs * 30.0) as i32 {
            g.tick(1.0 / 30.0, w, s, [false; 4]);
        }
    }
    fn hit(g: &mut Game, w: &mut FakeWorld, s: &Rec, key: u64, dmg: i32) {
        let e = w.enemies.iter_mut().find(|e| e.key == key).unwrap();
        e.hp = (e.hp - dmg).max(0);
        e.last_hit_by_player = true;
        g.tick(1.0 / 30.0, w, s, [false; 4]);
    }

    #[test]
    fn bo2_round_maths() {
        assert_eq!([1, 2, 3, 4, 5, 6, 10, 20].map(zombie_total), [6, 8, 13, 18, 24, 27, 33, 60]);
        assert_eq!([1, 2, 9, 10, 11].map(zombie_health), [150, 250, 950, 1045, 1149]);
        assert!((health_multiplier(1) - 1.0).abs() < 1e-6);
        assert!((health_multiplier(2) - (1.0 + 0.25 * (250.0 / 150.0 - 1.0))).abs() < 1e-6);
    }

    #[test]
    fn round_one_starts_after_load_in() {
        let (mut g, mut w, s) = (Game::default(), FakeWorld::default(), Rec::default());
        run(&mut g, &mut w, &s, 1.0);
        assert_eq!(g.phase, Phase::WaitingForWorld, "no player yet");
        w.player = Some(player());
        run(&mut g, &mut w, &s, FIRST_ROUND_DELAY as f32 - 0.5);
        assert_eq!(g.phase, Phase::PreRound);
        run(&mut g, &mut w, &s, 1.0);
        assert_eq!((g.phase, g.round, g.needed, g.points), (Phase::InRound, 1, 6, STARTING_POINTS));
        assert_eq!(s.0.borrow().as_slice(), &[Snd::RoundStart]);
        assert_eq!(g.hud().message.as_deref(), Some("ROUND 1"));
    }

    #[test]
    fn kills_score_and_finish_the_round() {
        let (mut g, mut w, s) = (Game::default(), FakeWorld::default(), Rec::default());
        w.player = Some(player());
        run(&mut g, &mut w, &s, FIRST_ROUND_DELAY as f32 + 0.5);
        w.enemies = (1..=6).map(|k| enemy(k, 300)).collect();
        g.tick(1.0 / 30.0, &mut w, &s, [false; 4]); // round 1: x1.0, so hp stays 300
        assert!(w.enemies.iter().all(|e| e.hp == 300));
        for k in 1..=6 {
            hit(&mut g, &mut w, &s, k, 100);
            hit(&mut g, &mut w, &s, k, 200);
        }
        assert_eq!(g.kills, 6);
        assert_eq!(g.points, STARTING_POINTS + 6 * (2 * SCORE_DAMAGE + SCORE_KILL));
        assert_eq!(g.phase, Phase::BetweenRounds);
        assert!(s.0.borrow().contains(&Snd::RoundOver));
        run(&mut g, &mut w, &s, ZOMBIE_BETWEEN_ROUND_TIME as f32 + 0.2);
        assert_eq!((g.phase, g.round, g.needed), (Phase::InRound, 2, 8));
        let jingles = [Snd::JuggJingle, Snd::SpeedJingle, Snd::DoubletapJingle, Snd::ReviveJingle];
        assert_eq!(s.0.borrow().iter().filter(|x| jingles.contains(x)).count(), 1, "one machine jingle in the break");
    }

    #[test]
    fn horde_is_tougher_in_round_two_and_others_kills_dont_count() {
        let (mut g, mut w, s) = (Game::default(), FakeWorld::default(), Rec::default());
        w.player = Some(player());
        run(&mut g, &mut w, &s, FIRST_ROUND_DELAY as f32 + 0.5);
        g.kills = g.needed; // finish round 1
        run(&mut g, &mut w, &s, ZOMBIE_BETWEEN_ROUND_TIME as f32 + 0.5);
        assert_eq!(g.round, 2);
        let mut far = enemy(2, 400);
        far.pos = [500.0, 0.0, 0.0];
        w.enemies = vec![enemy(1, 400), far];
        g.tick(1.0 / 30.0, &mut w, &s, [false; 4]);
        let want = (400.0 * health_multiplier(2)).round() as i32;
        assert_eq!((w.enemies[0].hp, w.enemies[0].max_hp), (want, want));
        assert_eq!(w.enemies[1].hp, 400, "outside the horde radius");
        g.tick(1.0 / 30.0, &mut w, &s, [false; 4]);
        assert_eq!(w.enemies[0].hp, want, "scaled once per round");
        // A kill by someone else (a spirit ash, a fall) gives nothing.
        let before = g.points;
        w.enemies[0].hp = 0;
        g.tick(1.0 / 30.0, &mut w, &s, [false; 4]);
        assert_eq!((g.points, g.kills), (before, 0));
    }

    #[test]
    fn buying_perks() {
        let (mut g, mut w, s) = (Game::default(), FakeWorld::default(), Rec::default());
        w.player = Some(player());
        run(&mut g, &mut w, &s, 1.0);
        s.0.borrow_mut().clear();
        g.tick(1.0 / 30.0, &mut w, &s, [true, false, false, false]); // Juggernog 2500 with 500
        assert!(!g.owned[0]);
        assert_eq!(g.hud().message.as_deref(), Some("NOT ENOUGH POINTS"));
        g.tick(1.0 / 30.0, &mut w, &s, [false, false, false, true]); // Quick Revive 500
        assert!(g.owned[3] && g.points == 0);
        run(&mut g, &mut w, &s, 2.5);
        assert!(w.no_dead, "Quick Revive arms the game's no-death flag");
        assert_eq!(s.0.borrow().as_slice(), &[Snd::Deny, Snd::Purchase, Snd::ReviveSting, Snd::Bump]);
        g.tick(1.0 / 30.0, &mut w, &s, [false, false, false, true]);
        g.tick(1.0 / 30.0, &mut w, &s, [false, false, false, false]);
        assert_eq!(s.0.borrow().last(), Some(&Snd::Deny), "already owned");
    }

    #[test]
    fn juggernog_and_quick_revive() {
        let (mut g, mut w, s) = (Game::default(), FakeWorld::default(), Rec::default());
        w.player = Some(player());
        run(&mut g, &mut w, &s, 1.0);
        g.points = 10_000;
        g.buy(0, &s);
        g.tick(1.0 / 30.0, &mut w, &s, [false; 4]);
        assert_eq!(g.hud().jugg_pool, 1500, "BO2 100 -> 250 health: +1.5x max");
        assert!(w.no_dead);
        w.player.as_mut().unwrap().hp = 500; // a 500 hit, fully absorbed
        g.tick(1.0 / 30.0, &mut w, &s, [false; 4]);
        assert_eq!((w.player.unwrap().hp, g.hud().jugg_pool), (1000, 1000));
        w.player.as_mut().unwrap().hp = 1; // a lethal hit stopped at 1 HP
        g.tick(1.0 / 30.0, &mut w, &s, [false; 4]);
        assert_eq!((w.player.unwrap().hp, g.hud().jugg_pool), (1000, 1));
        w.player.as_mut().unwrap().hp = 1;
        g.tick(1.0 / 30.0, &mut w, &s, [false; 4]);
        assert_eq!((w.player.unwrap().hp, g.hud().jugg_pool), (2, 0));
        assert!(!w.no_dead, "pool dry and no Quick Revive: the next lethal hit kills");
        g.buy(3, &s);
        g.tick(1.0 / 30.0, &mut w, &s, [false; 4]);
        assert!(w.no_dead);
        w.player.as_mut().unwrap().hp = 1;
        g.tick(1.0 / 30.0, &mut w, &s, [false; 4]);
        assert_eq!(w.player.unwrap().hp, 1000);
        assert!(!w.no_dead && g.owned == [false; 4] && g.stats.revives == 1);
        assert_ne!(g.phase, Phase::Dead);
        // Jugg pool refills at the next round start.
        g.buy(0, &s);
        g.tick(1.0 / 30.0, &mut w, &s, [false; 4]);
        w.player.as_mut().unwrap().hp = 400;
        g.tick(1.0 / 30.0, &mut w, &s, [false; 4]);
        assert_eq!(g.hud().jugg_pool, 900);
        run(&mut g, &mut w, &s, FIRST_ROUND_DELAY as f32);
        assert_eq!(g.hud().jugg_pool, 1500);
    }

    #[test]
    fn double_tap_doubles_damage_but_game_lands_the_kill() {
        let (mut g, mut w, s) = (Game::default(), FakeWorld::default(), Rec::default());
        w.player = Some(player());
        run(&mut g, &mut w, &s, 1.0); // pre-round: no scaling
        g.points = 5000;
        g.buy(2, &s);
        w.enemies = vec![enemy(7, 300)];
        g.tick(1.0 / 30.0, &mut w, &s, [false; 4]);
        hit(&mut g, &mut w, &s, 7, 100);
        assert_eq!(w.enemies[0].hp, 100);
        hit(&mut g, &mut w, &s, 7, 90);
        assert_eq!(w.enemies[0].hp, 1, "never kills by itself");
        hit(&mut g, &mut w, &s, 7, 50);
        assert_eq!(g.stats.kills, 1);
    }

    #[test]
    fn speed_cola_restores_stamina() {
        let (mut g, mut w, s) = (Game::default(), FakeWorld::default(), Rec::default());
        w.player = Some(player());
        run(&mut g, &mut w, &s, 1.0);
        g.points = 5000;
        g.buy(1, &s);
        w.player.as_mut().unwrap().stamina = 0;
        run(&mut g, &mut w, &s, 1.0);
        let st = w.player.unwrap().stamina;
        assert!((33..=36).contains(&st), "{st}");
    }

    #[test]
    fn death_is_game_over_then_a_new_game() {
        let (mut g, mut w, s) = (Game::default(), FakeWorld::default(), Rec::default());
        w.player = Some(player());
        run(&mut g, &mut w, &s, FIRST_ROUND_DELAY as f32 + 0.5);
        g.points = 4000;
        g.buy(0, &s);
        w.player.as_mut().unwrap().hp = 0;
        g.tick(1.0 / 30.0, &mut w, &s, [false; 4]);
        assert_eq!(g.phase, Phase::Dead);
        assert!(s.0.borrow().contains(&Snd::GameOver));
        assert_eq!(g.hud().message.as_deref(), Some("GAME OVER\nYOU SURVIVED 1 ROUND"));
        w.player = None; // loading screen
        run(&mut g, &mut w, &s, 3.0);
        w.player = Some(player());
        run(&mut g, &mut w, &s, 2.5);
        assert_eq!((g.phase, g.round, g.points, g.owned), (Phase::PreRound, 0, STARTING_POINTS, [false; 4]));
        run(&mut g, &mut w, &s, FIRST_ROUND_DELAY as f32 + 0.1);
        assert_eq!(g.round, 1);
    }
}
