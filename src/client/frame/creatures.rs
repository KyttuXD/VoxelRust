//! Drawing the creatures and other players (methods of `App`, moved out of app.rs).

#[allow(unused_imports)]
use super::*;

impl App {
    /// Draw every creature near the camera, and the player's own body when
    /// the camera is outside it.
    pub(super) fn draw_creatures(&self, r: &mut Renderer, g: &Session, cam: &Camera, tags: &mut Vec<(f32, f32, String)>) {
        let reg = reg();
        let look = crate::game::fx::sky::current(g);
        let (_, _, day) = crate::render::sky_colors(look.time_of(g.time));
        let day = look.light(day);
        // Light at a spot: full daylight under open sky, dimmer under cover.
        let light_at = |pos: [f64; 3]| -> f32 {
            let (x, z) = (floord(pos[0]), floord(pos[2]));
            let covered = (floord(pos[1]) + 2..CH).any(|y| reg.opaque[g.world.get(x, y, z) as usize] != 0);
            (0.18 + 0.82 * day) * if covered { 0.55 } else { 1.0 }
        };
        let rel = |pos: [f64; 3]| [(pos[0] - cam.pos[0]) as f32, (pos[1] - cam.pos[1]) as f32, (pos[2] - cam.pos[2]) as f32];
        // What is behind the camera is not drawn at all.
        let ahead = crate::math::forward(cam.yaw, cam.pitch);
        let behind = |p: [f32; 3], size: f32| p[0] * ahead.x + p[1] * ahead.y + p[2] * ahead.z < -size;
        for m in &g.mobs.list {
            let p = rel(m.body.pos);
            if p[0] * p[0] + p[2] * p[2] > 72.0 * 72.0 || behind(p, 4.0) {
                continue;
            }
            // A creature whose ground is not loaded stands still in mid-air: not drawn.
            if !g.world.is_loaded(floord(m.body.pos[0]), floord(m.body.pos[2])) {
                continue;
            }
            let d = &reg.entities[m.kind];
            let (model, colour, height) = m.look();
            // Things that glow are not dimmed by the dark.
            let l = light_at(m.body.pos).max(d.x.glow as f32 / 15.0);
            let mut tint = [0, 1, 2].map(|i| (colour[i] as f32 * l) as u8);
            if m.hurt > 0.0 {
                tint = [255, tint[1] / 3, tint[2] / 3];
            }
            let mut pose = crate::model::Pose { phase: m.phase, stride: m.stride, anim: d.x.anim, clock: m.clock, ..Default::default() };
            // A person standing still on a seat sits; in a bed, sleeps.
            let kind = crate::client::pose_view::creature_posture(&|x, y, z| g.world.get(x, y, z), m.body.pos, m.stride);
            let mut at = p;
            if kind != crate::content::pose::Kind::Stand {
                pose.body = crate::content::pose::Body::steady(kind);
                at = rel(crate::client::pose_view::creature_at(kind, m.body.pos, m.body.yaw));
            }
            r.tex_override = r.entity_texture(m.tex());
            r.draw_model(model, &pose, at, m.body.yaw, height, tint);
        }
        // Props: blocks that are models (furniture, statues, plants). Only
        // those near enough to matter, and not the ones behind the camera.
        let (pcx, pcz) = crate::world::chunk_at(cam.pos[0], cam.pos[2]);
        let still = crate::model::Pose::default();
        for dx in -3..=3 {
            for dz in -3..=3 {
                let (cx, cz) = (pcx + dx, pcz + dz);
                let chunk = match g.world.chunks.get(&(cx, cz)) {
                    Some(c) if !c.props.is_empty() => c,
                    _ => continue,
                };
                for &(at, b) in &chunk.props {
                    let d = block::def(b);
                    let (model, height, lift) = match d.x.prop {
                        Some(p) => p,
                        None => continue,
                    };
                    let pos = [(cx * 16 + at[0] as i32) as f64 + 0.5, at[1] as f64 + lift as f64, (cz * 16 + at[2] as i32) as f64 + 0.5];
                    let p = rel(pos);
                    if p[0] * p[0] + p[2] * p[2] > 44.0 * 44.0 || behind(p, 3.0) {
                        continue;
                    }
                    // Placed facing the player: each state is a quarter turn.
                    let turn = ((b - d.first) % d.rotations.max(1) as u16) as f32;
                    let l = (light_at([pos[0], pos[1], pos[2]]) * 255.0) as u8;
                    r.draw_model(model, &still, p, turn * PI * 0.5, height, [l, l, l]);
                }
            }
        }
        let mut sparks: Vec<([f32; 3], [u8; 3])> = g.shots.iter().map(|s| (rel(s.pos), s.colour)).collect();
        // The sparks of fuses and explosions.
        // (the particles of mods, kinds 1 and up, are drawn by `Renderer::draw_effects`)
        sparks.extend(g.primed.sparks.iter().filter(|s| s.kind == 0).map(|s| (rel(s.pos), s.colour)));
        r.draw_sparks(&sparks);
        // Things lying on the ground are the things themselves, turning slowly.
        let white = crate::client::hand::white_tile();
        let mut lying = Vec::new();
        for d in &g.drops {
            let held = crate::client::hand::held_of(Some(d.item as usize));
            let spin = g.clock * 1.6 + d.pos[0] as f32;
            let bob = (sinf(g.clock * 2.5 + d.pos[2] as f32) * 0.04) as f32;
            crate::client::hand::dropped(held, white, rel([d.pos[0], d.pos[1] + 0.3, d.pos[2]]), spin + bob, &mut lying);
        }
        // Lit TNT, flashing white.
        crate::client::hand::primed_blocks(&g.primed.list, white, &rel, &mut lying);
        // Blocks a mod threw.
        if let Some(fx) = crate::game::fx::state_ref(g) {
            crate::client::hand::falling_blocks(&fx.falling, &rel, &mut lying);
        }
        r.draw_quads(&lying, None);
        // Whatever the mods' own code has to show.
        {
            let view = crate::native::View { eye: cam.pos, yaw: cam.yaw, pitch: cam.pitch, third_person: self.third_person, light: &light_at };
            for (_, n) in crate::native::active() {
                if let Some(draw) = n.draw {
                    draw(g, r, &view);
                }
            }
            crate::script::ext::draw::draw_world(g, r, cam.pos, self.third_person, &light_at);
        }
        // Other players, with their names over their heads.
        if let Some(net) = &g.net {
            for (id, p) in net.players.iter().filter(|p| p.1.seen) {
                let at = rel(p.pos);
                if at[0] * at[0] + at[2] * at[2] > 96.0 * 96.0 {
                    continue;
                }
                let slim = self.remote_slim.get(id).copied().unwrap_or(false);
                let body = body_for(g, *id, slim);
                let model = match reg.model(arm_models(&body, slim).0) {
                    Some(model) => model,
                    None => continue,
                };
                let l = (light_at(p.pos) * 255.0) as u8;
                let drawn = crate::client::pose_view::remote(p, g.clock);
                // A player whose skin has not arrived wears the built-in one their name picks.
                r.tex_override = match self.remote_skins.get(id) {
                    Some(t) => *t,
                    None => r.entity_texture(&crate::pack_entities::skin_picture(crate::pack_entities::default_skin_for(&p.name))),
                };
                r.draw_model(model, &drawn.pose, rel(drawn.at), drawn.yaw, crate::player::HEIGHT as f32, [l, l, l]);
                // What they hold, out to the right of the body (not while sitting, lying or emoting: the hand is elsewhere).
                let busy = drawn.pose.body.walk_weight() < 0.5 || drawn.pose.body.emote != 0;
                let held = if p.held == 0 || busy { crate::client::hand::Held::Nothing } else { crate::client::hand::held_of(Some(p.held as usize - 1)) };
                let mut held_quads = Vec::new();
                crate::client::hand::on_body(held, crate::client::hand::white_tile(), 0.0, at, p.yaw, crate::player::HEIGHT as f32, &mut held_quads);
                r.draw_quads(&held_quads, None);
                if let Some((sx, sy)) = r.project(cam, [at[0], at[1] + 2.15, at[2]]) {
                    // Their strongest news under the name: an effect they have (and how many more).
                    let mut label = p.name.to_ascii_uppercase();
                    if let Some(d) = p.effects.first().and_then(|e| reg.fx.get(&e.0)) {
                        label.push_str(&alloc::format!(" ({}{})", d.name.to_ascii_uppercase(), if p.effects.len() > 1 { alloc::format!(" +{}", p.effects.len() - 1) } else { String::new() }));
                    }
                    tags.push((sx, sy, label));
                }
            }
        }
        if self.third_person {
            let body = body_for(g, own_id(g), self.settings.slim_arms);
            if let Some(model) = reg.model(arm_models(&body, self.settings.slim_arms).0) {
                let drawn = crate::client::pose_view::local(g);
                let l = (light_at(g.player.pos) * 255.0) as u8;
                r.tex_override = self.skin_tex;
                r.draw_model(model, &drawn.pose, rel(drawn.at), drawn.yaw, crate::player::HEIGHT as f32, [l, l, l]);
                let busy = drawn.pose.body.walk_weight() < 0.5 || drawn.pose.body.emote != 0;
                let held = if busy { crate::client::hand::Held::Nothing } else { crate::client::hand::held_of(g.held().map(|h| h.0)) };
                let mut held_quads = Vec::new();
                crate::client::hand::on_body(held, crate::client::hand::white_tile(), hand_swing(g), rel(g.player.pos), g.player.yaw, crate::player::HEIGHT as f32, &mut held_quads);
                r.draw_quads(&held_quads, None);
            }
        }
    }
}
