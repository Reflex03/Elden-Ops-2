//! BO2-style HUD drawn over Elden Ring with hudhook/ImGui (hud.json).

use crate::game::HudSnapshot;
use crate::sheets::*;
use hudhook::{ImguiRenderLoop, RenderContext};
use imgui::{Context, FontConfig, FontId, FontSource, Ui};
use std::sync::Mutex;

pub static HUD: Mutex<Option<HudSnapshot>> = Mutex::new(None);

const SIZES: [f32; 5] = [HUD_ROUND.font_px, HUD_MESSAGE.font_px, HUD_POINTS.font_px, HUD_PERKS.font_px, HUD_SHOP.font_px];

#[derive(Default)]
pub struct Overlay {
    fonts: Vec<(f32, FontId)>,
    reported: bool,
}

// The overlay only lives on the game's render thread; FontIds are plain handles into its atlas.
unsafe impl Send for Overlay {}
unsafe impl Sync for Overlay {}

impl Overlay {
    fn font(&self, px: f32) -> Option<FontId> {
        self.fonts.iter().find(|(s, _)| *s == px).map(|(_, f)| *f)
    }
}

fn col(c: [f32; 4]) -> [f32; 4] {
    c
}

impl ImguiRenderLoop for Overlay {
    fn initialize<'a>(&'a mut self, ctx: &mut Context, _rc: &'a mut dyn RenderContext) {
        ctx.set_ini_filename(None);
        // A heavy condensed face from Windows itself; ImGui's own font if it isn't there.
        let ttf = ["C:\\Windows\\Fonts\\impact.ttf", "C:\\Windows\\Fonts\\arialbd.ttf"].iter().find_map(|p| std::fs::read(p).ok());
        for px in SIZES {
            if self.font(px).is_some() {
                continue;
            }
            let id = match &ttf {
                Some(data) => ctx.fonts().add_font(&[FontSource::TtfData { data, size_pixels: px, config: Some(FontConfig { oversample_h: 2, ..Default::default() }) }]),
                None => ctx.fonts().add_font(&[FontSource::DefaultFontData { config: Some(FontConfig { size_pixels: px, ..Default::default() }) }]),
            };
            self.fonts.push((px, id));
        }
    }

    fn render(&mut self, ui: &mut Ui) {
        let mut s = HUD.lock().ok().and_then(|h| h.clone()).unwrap_or_default();
        if !s.visible {
            if !crate::melty_test::active() {
                return;
            }
            // Test run on the title screen: show the HUD so the run can see it drawn.
            s.visible = true;
            s.message = Some("ELDEN OPS\nLOAD A CHARACTER: ROUND 1 STARTS AT THE GRACE".into());
        }
        if !self.reported {
            self.reported = true;
            crate::melty_test::report("hud-drawn", true, None);
        }
        let [w, h] = ui.io().display_size;
        let dl = ui.get_foreground_draw_list();
        let text = |row: &HudRow, t: &str, shadow: bool| {
            let tok = self.font(row.font_px).map(|f| ui.push_font(f));
            let size = ui.calc_text_size(t);
            let [ox, oy] = row.offset;
            let pos = match row.anchor {
                Anchor::BottomLeft => [ox, h + oy - size[1]],
                Anchor::BottomRight => [w + ox - size[0], h + oy - size[1]],
                Anchor::TopCenter => [(w - size[0]) / 2.0 + ox, oy],
            };
            if shadow {
                dl.add_text([pos[0] + 2.0, pos[1] + 2.0], [0.0, 0.0, 0.0, 0.8], t);
            }
            dl.add_text(pos, col(row.color), t);
            drop(tok);
        };
        if s.round > 0 {
            text(&HUD_ROUND, &s.round.to_string(), true);
        }
        text(&HUD_POINTS, &s.points.to_string(), true);
        if !s.owned.is_empty() {
            let mut line = s.owned.join("  ·  ");
            if s.jugg_pool > 0 {
                line.push_str(&format!("   (+{} HP)", s.jugg_pool));
            }
            text(&HUD_PERKS, &line, true);
        }
        let shop: Vec<String> = s.shop.iter().filter(|(_, _, _, owned)| !owned).map(|(k, n, c, _)| format!("[{k}] {n}  {c}")).collect();
        if !shop.is_empty() {
            text(&HUD_SHOP, &shop.join("\n"), true);
        }
        if let Some(m) = &s.message {
            text(&HUD_MESSAGE, m, true);
        }
    }
}
