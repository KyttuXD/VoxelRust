//! Keys and the event loop (methods of `App`, moved out of app.rs).

#[allow(unused_imports)]
use super::*;

impl App {
    pub(super) fn key_down(&mut self, r: &Renderer, inp: &mut UiInput, sc: i32, repeat: bool) {
        if let Some(i) = self.rebinding.take() {
            if sc != key::ESCAPE {
                self.settings.keys[i] = sc;
            }
            return;
        }
        // Ctrl (Cmd on a Mac) with A, C, X or V works on the text box that
        // has the keyboard.
        let command = [224usize, 228, 227, 231].iter().any(|&k| self.keys[k]);
        if command && self.focus != 0 {
            match sc {
                4 => inp.keys.push(ui::KEY_SELECT_ALL),
                6 => inp.keys.push(ui::KEY_COPY),
                27 => inp.keys.push(ui::KEY_CUT),
                25 => inp.paste.push_str(&sys::clipboard()),
                _ => {}
            }
            return;
        }
        inp.keys.push(sc);
        if repeat {
            return;
        }
        match sc {
            key::F2 => return self.save_screenshot(r),
            key::F3 => {
                self.show_debug = !self.show_debug;
                return;
            }
            key::F11 => return self.toggle_fullscreen(),
            _ => {}
        }
        let typing = self.focus != 0;
        let (k_inv, k_fly, k_map) = (self.settings.key(Action::Inventory), self.settings.key(Action::Fly), self.settings.key(Action::Map));
        let k_waypoints = self.settings.key(Action::Waypoints);
        let k_chat = self.settings.key(Action::Chat);
        match self.screen {
            Screen::Game => {
                // Jump twice quickly to take off or land (creative flying).
                if sc == self.settings.key(Action::Jump) && self.session.as_ref().map_or(false, |g| g.mode == GameMode::Creative) {
                    let now = sys::now();
                    if now - self.last_jump_tap < 0.3 {
                        self.toggle_fly = true;
                        self.last_jump_tap = 0.0;
                    } else {
                        self.last_jump_tap = now;
                    }
                }
                if sc == key::ESCAPE {
                    self.set_screen(Screen::Pause);
                } else if sc == k_inv {
                    self.set_screen(Screen::Inventory);
                } else if sc == k_map {
                    self.open_map();
                } else if sc == k_waypoints {
                    self.open_full_map();
                } else if sc == self.settings.key(Action::Drop) {
                    self.drop_item = true;
                } else if sc == k_chat {
                    self.form_chat.clear();
                    self.set_screen(Screen::Chat);
                    self.focus = 20;
                    // The key that opened the box must not be typed into it.
                    inp.text.clear();
                    self.swallow_text = true;
                } else if sc == k_fly {
                    self.leave = true;
                } else if sc == self.settings.key(Action::Emote) && !typing && self.session.as_ref().map_or(false, |g| !g.dead) {
                    self.wheel.open();
                    self.set_screen(Screen::Emote);
                } else if sc == key::F5 {
                    // F5 cycles: first person, behind, in front.
                    if !self.third_person {
                        self.third_person = true;
                        self.third_front = false;
                    } else if !self.third_front {
                        self.third_front = true;
                    } else {
                        self.third_person = false;
                        self.third_front = false;
                    }
                } else if sc == key::F1 {
                    self.hud_hidden = !self.hud_hidden;
                } else if (key::N1..=key::N9).contains(&sc) {
                    if let Some(g) = self.session.as_mut() {
                        g.selected = (sc - key::N1) as usize;
                    }
                }
            }
            Screen::Inventory | Screen::Craft | Screen::Chest => {
                if sc == key::ESCAPE || (sc == k_inv && !typing) {
                    self.set_screen(Screen::Game);
                } else if !typing && (key::N1..=key::N9).contains(&sc) {
                    if let Some(g) = self.session.as_mut() {
                        g.selected = (sc - key::N1) as usize;
                    }
                }
            }
            Screen::Pause if sc == key::ESCAPE => self.set_screen(Screen::Game),
            Screen::Chat if sc == key::ESCAPE => self.set_screen(Screen::Game),
            Screen::Join if sc == key::ESCAPE && !typing => self.set_screen(Screen::Title),
            Screen::EditWorld if sc == key::ESCAPE && !typing => self.set_screen(Screen::Worlds),
            Screen::SkinUrl if sc == key::ESCAPE => self.set_screen(Screen::Settings),
            Screen::Picker if sc == key::ESCAPE => {
                self.close_picker(None);
            }
            Screen::AddServer if sc == key::ESCAPE => self.set_screen(Screen::Join),
            Screen::Map if sc == key::ESCAPE || sc == k_map => self.set_screen(Screen::Game),
            Screen::FullMap if !typing && (sc == key::ESCAPE || sc == k_map || sc == k_waypoints) => self.close_full_map(),
            Screen::Settings if sc == key::ESCAPE && !typing => self.leave_settings(),
            Screen::NewWorld if sc == key::ESCAPE => self.set_screen(Screen::Worlds),
            Screen::Worlds if sc == key::ESCAPE => self.set_screen(Screen::Title),
            _ => {}
        }
    }

    pub(super) fn handle_events(&mut self, r: &mut Renderer, inp: &mut UiInput) {
        let api = sdl::sdl();
        let mut ev = sdl::Event::new();
        inp.clear_frame();
        while unsafe { (api.SDL_PollEvent)(&mut ev) } != 0 {
            match ev.u32_at(0) {
                sdl::EV_QUIT => self.quit = true,
                sdl::EV_WINDOW => match ev.u8_at(12) {
                    sdl::WINEV_SIZE_CHANGED => {
                        r.w = ev.i32_at(16).max(1);
                        r.h = ev.i32_at(20).max(1);
                    }
                    sdl::WINEV_FOCUS_LOST => {
                        if self.screen == Screen::Game && !self.scripted {
                            self.set_screen(Screen::Pause);
                        }
                        self.keys = [false; 512];
                    }
                    _ => {}
                },
                sdl::EV_KEYDOWN => {
                    let sc = ev.i32_at(16);
                    if (0..512).contains(&sc) {
                        self.keys[sc as usize] = true;
                        self.key_down(r, inp, sc, ev.u8_at(13) != 0);
                    }
                }
                sdl::EV_KEYUP => {
                    let sc = ev.i32_at(16);
                    if (0..512).contains(&sc) {
                        self.keys[sc as usize] = false;
                    }
                }
                sdl::EV_DROPFILE => {
                    // A picture dropped on the window: the pointer at offset 8, freed with SDL_free.
                    let p = ev.ptr_at(8) as *mut core::ffi::c_char;
                    if !p.is_null() {
                        let path = unsafe { sys::from_cstr(p as *const core::ffi::c_char) };
                        unsafe { (api.SDL_free)(p as *mut core::ffi::c_void) };
                        self.skin_dropped(r, &path);
                    }
                }
                sdl::EV_TEXTINPUT if self.swallow_text => self.swallow_text = false,
                sdl::EV_TEXTINPUT if [224usize, 228, 227, 231].iter().any(|&k| self.keys[k]) => {}
                sdl::EV_TEXTINPUT => {
                    // A NUL-terminated UTF-8 string at offset 12.
                    for &b in ev.0[12..44].iter().take_while(|&&b| b != 0) {
                        if b.is_ascii() {
                            inp.text.push(b as char);
                        }
                    }
                }
                sdl::EV_MOUSEMOTION => {
                    inp.mouse = (ev.i32_at(20) as f32, ev.i32_at(24) as f32);
                    inp.moved.0 += ev.i32_at(28) as f32;
                    inp.moved.1 += ev.i32_at(32) as f32;
                    if self.screen == Screen::Game && !self.scripted {
                        if let Some(g) = self.session.as_mut() {
                            let k = self.settings.sens as f32 * 0.0005;
                            let dy = if self.settings.invert_y { -ev.i32_at(32) } else { ev.i32_at(32) };
                            g.player.yaw -= ev.i32_at(28) as f32 * k;
                            g.player.pitch = (g.player.pitch - dy as f32 * k).clamp(-1.55, 1.55);
                            // Keep the angle small so precision never degrades.
                            if g.player.yaw > PI {
                                g.player.yaw -= TAU;
                            } else if g.player.yaw < -PI {
                                g.player.yaw += TAU;
                            }
                        }
                    }
                }
                sdl::EV_MOUSEDOWN => {
                    inp.mouse = (ev.i32_at(20) as f32, ev.i32_at(24) as f32);
                    let b = ev.u8_at(16) as usize;
                    if b < 4 {
                        if self.screen == Screen::Game {
                            self.buttons[b] = true;
                            if b == 2 {
                                self.pick = true;
                            }
                        } else {
                            inp.clicked[b] = true;
                            inp.down[b] = true;
                        }
                    }
                }
                sdl::EV_MOUSEUP => {
                    let b = ev.u8_at(16) as usize;
                    if b < 4 {
                        self.buttons[b] = false;
                        inp.down[b] = false;
                    }
                }
                sdl::EV_MOUSEWHEEL => {
                    let dy = ev.i32_at(20).signum();
                    if self.screen == Screen::Game {
                        if let Some(g) = self.session.as_mut() {
                            g.selected = (g.selected as i32 - dy).rem_euclid(HOTBAR as i32) as usize;
                        }
                    } else {
                        inp.wheel += dy;
                    }
                }
                _ => {}
            }
        }
    }
}
