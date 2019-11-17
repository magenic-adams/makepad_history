use render::*;
use crate::buttonlogic::*;
use crate::tabclose::*;
use crate::widgettheme::*;

#[derive(Clone)]
pub struct Tab {
    pub bg_layout: LayoutId,
    pub bg: Quad,
    pub text: Text,
    pub tab_close: TabClose,
    pub label: String,
    pub is_closeable: bool,
    pub animator: Animator,
    pub z: f32,
    pub abs_origin: Option<Vec2>,
    pub _is_selected: bool,
    pub _is_focussed: bool,
    pub _bg_area: Area,
    pub _bg_inst: Option<InstanceArea>,
    pub _text_area: Area,
    pub _close_anim_rect: Rect,
    pub _is_down: bool,
    pub _is_drag: bool
}

#[derive(Clone, PartialEq)]
pub enum TabEvent {
    None,
    DragMove(FingerMoveEvent),
    DragEnd(FingerUpEvent),
    Closing,
    Close,
    Select,
}

instance_color!(Tab_border_color);
instance_float!(Tab_closing);

impl Tab {
    pub fn style(cx: &mut Cx) -> Self {
        let mut tab = Self {
            label: "Tab".to_string(),
            is_closeable: true,
            z: 0.,
            bg: Quad {
                shader: cx.add_shader(Self::def_bg_shader(), "Tab.bg"),
                ..Quad::style(cx)
            },
            bg_layout: Layout_tab::id(),
            tab_close: TabClose::style(cx),
            text: Text::style(cx, TextStyle_tab_title::id()),
            animator: Animator::new_no_default(),
            abs_origin: None,
            _is_selected: false,
            _is_focussed: false,
            _is_down: false,
            _is_drag: false,
            _close_anim_rect: Rect::zero(),
            _text_area: Area::Empty,
            _bg_area: Area::Empty,
            _bg_inst: None,
        };
        tab.animator.set_anim_as_last_values(&tab.anim_default(cx));
        tab
    }
    
    pub fn get_bg_color(&self) -> ColorId {
        if self._is_selected {
            Color_bg_selected::id()
        }
        else {
            Color_bg_normal::id()
        }
    }
    
    pub fn get_text_color(&self) -> ColorId {
        if self._is_selected {
            if self._is_focussed {
                Color_text_selected_focus::id()
            }
            else {
                Color_text_selected_defocus::id()
            }
        }
        else {
            if self._is_focussed {
                Color_text_deselected_focus::id()
            }
            else {
                Color_text_deselected_defocus::id()
            }
        }
    }
    
    pub fn anim_default(&self, cx: &Cx) -> Anim {
        Anim::new(Play::Cut {duration: 0.05}, vec![
            Track::color_id(Quad_color::id(), Ease::Lin, vec![(1.0, self.get_bg_color())]),
            Track::color_id(Tab_border_color::id(), Ease::Lin, vec![(1.0, Color_bg_selected::id())]),
            Track::color_id(Text_color::id(), Ease::Lin, vec![(1.0, self.get_text_color())]),
            //Track::color_id(cx, "icon.color", Ease::Lin, vec![(1.0, self.get_text_color(cx))])
        ])
    }
    
    pub fn anim_over(&self, cx: &Cx) -> Anim {
        Anim::new(Play::Cut {duration: 0.01}, vec![
            Track::color_id(Quad_color::id(), Ease::Lin, vec![(1.0, self.get_bg_color())]),
            Track::color_id(Tab_border_color::id(), Ease::Lin, vec![(1.0, Color_bg_selected::id())]),
            Track::color_id(Text_color::id(), Ease::Lin, vec![(1.0, self.get_text_color())]),
            //Track::color_id(cx, "icon.color", Ease::Lin, vec![(1.0, self.get_text_color(cx))])
        ])
    }
    
    pub fn anim_down(&self, cx: &Cx) -> Anim {
        Anim::new(Play::Cut {duration: 0.01}, vec![
            Track::color_id(Quad_color::id(), Ease::Lin, vec![(1.0, self.get_bg_color())]),
            Track::color_id(Tab_border_color::id(), Ease::Lin, vec![(1.0, Color_bg_selected::id())]),
            Track::color_id(Text_color::id(), Ease::Lin, vec![(1.0, self.get_text_color())]),
           // Track::color_id(cx, "icon.color", Ease::Lin, vec![(1.0, self.get_text_color(cx))])
        ])
    }
    
    pub fn anim_close(&self, _cx: &Cx) -> Anim {
        Anim::new(Play::Single {duration: 0.1, cut: true, term: true, end: 1.0}, vec![
            Track::float(Tab_closing::id(), Ease::OutExp, vec![(0.0, 1.0), (1.0, 0.0)]),
        ])
    }
    
    pub fn def_bg_shader() -> ShaderGen {
        Quad::def_quad_shader().compose(shader_ast!({
            
            let border_color: Tab_border_color;
            const border_width: float = 1.0;
            
            fn pixel() -> vec4 {
                df_viewport(pos * vec2(w, h));
                df_rect(-1., -1., w + 2., h + 2.);
                df_fill(color);
                df_move_to(w, 0.);
                df_line_to(w, h);
                df_move_to(0., 0.);
                df_line_to(0., h);
                return df_stroke(border_color, 1.);
            }
        }))
    }
    
    pub fn set_tab_focus(&mut self, cx: &mut Cx, focus: bool) {
        if focus != self._is_focussed {
            self._is_focussed = focus;
            self.animator.play_anim(cx, self.anim_default(cx));
        }
    }
    
    pub fn set_tab_selected(&mut self, cx: &mut Cx, selected: bool) {
        if selected != self._is_selected {
            self._is_selected = selected;
            self.animator.play_anim(cx, self.anim_default(cx));
        }
    }
    
    pub fn set_tab_state(&mut self, cx: &mut Cx, selected: bool, focus: bool) {
        self._is_selected = selected;
        self._is_focussed = focus;
        self.animator.set_anim_as_last_values(&self.anim_default(cx));
    }
    
    pub fn handle_tab(&mut self, cx: &mut Cx, event: &mut Event) -> TabEvent {
        
        if !self.animator.term_anim_playing() {
            match self.tab_close.handle_tab_close(cx, event) {
                ButtonEvent::Down => {
                    self._close_anim_rect = self._bg_area.get_rect(cx, false);
                    self.animator.play_anim(cx, self.anim_close(cx));
                    return TabEvent::Closing;
                },
                _ => ()
            }
        }
        
        match event.hits(cx, self._bg_area, HitOpt::default()) {
            Event::Animate(ae) => {
                // its playing the term anim, run a redraw
                if self.animator.term_anim_playing() {
                    self.animator.calc_float(cx, Tab_closing::id(), ae.time);
                    cx.redraw_child_area(self._bg_area);
                }
                else {
                    self.animator.write_area(cx, self._bg_area, ae.time);
                    self.animator.write_area(cx, self._text_area, ae.time);
                }
            },
            Event::AnimEnded(_ae) => {
                if self.animator.term_anim_playing() {
                    return TabEvent::Close;
                }
                else {
                    self.animator.end();
                }
            },
            Event::FingerDown(_fe) => {
                if self.animator.term_anim_playing() {
                    return TabEvent::None
                }
                cx.set_down_mouse_cursor(MouseCursor::Hand);
                self._is_down = true;
                self._is_drag = false;
                self._is_selected = true;
                self._is_focussed = true;
                self.animator.play_anim(cx, self.anim_down(cx));
                return TabEvent::Select;
            },
            Event::FingerHover(fe) => {
                cx.set_hover_mouse_cursor(MouseCursor::Hand);
                match fe.hover_state {
                    HoverState::In => {
                        if self._is_down {
                            self.animator.play_anim(cx, self.anim_down(cx));
                        }
                        else {
                            self.animator.play_anim(cx, self.anim_over(cx));
                        }
                    },
                    HoverState::Out => {
                        self.animator.play_anim(cx, self.anim_default(cx));
                    },
                    _ => ()
                }
            },
            Event::FingerUp(fe) => {
                self._is_down = false;
                
                if fe.is_over {
                    if !fe.is_touch {
                        self.animator.play_anim(cx, self.anim_over(cx));
                    }
                    else {
                        self.animator.play_anim(cx, self.anim_default(cx));
                    }
                }
                else {
                    self.animator.play_anim(cx, self.anim_default(cx));
                }
                if self._is_drag {
                    self._is_drag = false;
                    return TabEvent::DragEnd(fe);
                }
            },
            Event::FingerMove(fe) => {
                if !self._is_drag {
                    if fe.move_distance() > 10. {
                        //cx.set_down_mouse_cursor(MouseCursor::Hidden);
                        self._is_drag = true;
                    }
                }
                if self._is_drag {
                    return TabEvent::DragMove(fe);
                }
                //self.animator.play_anim(cx, self.animator.default.clone());
            },
            _ => ()
        };
        TabEvent::None
    }
    
    pub fn get_tab_rect(&mut self, cx: &Cx) -> Rect {
        self._bg_area.get_rect(cx, false)
    }
    
    pub fn begin_tab(&mut self, cx: &mut Cx) -> Result<(), ()> {
        // pull the bg color from our animation system, uses 'default' value otherwise
        self.bg.z = self.z;
        self.bg.color = self.animator.last_color(cx, Quad_color::id());
        
        // check if we are closing
        if self.animator.term_anim_playing() {
            // so so BUT how would we draw this thing with its own clipping
            let bg_inst = self.bg.draw_quad(
                cx,
                Walk::wh(
                    Width::Fix(self._close_anim_rect.w * self.animator.last_float(cx, Tab_closing::id())),
                    Height::Fix(self._close_anim_rect.h),
                )
            );
            bg_inst.push_last_color(cx, &self.animator, Tab_border_color::id());
            self._bg_area = bg_inst.into_area();
            self.animator.update_area_refs(cx, self._bg_area);
            return Err(())
        }
        else {
            let layout = if let Some(abs_origin) = self.abs_origin {
                Layout {abs_origin: Some(abs_origin), ..self.bg_layout.get(cx)}
            }
            else {
                self.bg_layout.get(cx)
            };
            let bg_inst = self.bg.begin_quad(cx, layout);
            bg_inst.push_last_color(cx, &self.animator, Tab_border_color::id());
            if self.is_closeable {
                self.tab_close.draw_tab_close(cx);
                cx.turtle_align_y();
            }
            // push the 2 vars we added to bg shader
            self.text.z = self.z;
            self.text.color = self.animator.last_color(cx, Text_color::id());
            self._text_area = self.text.draw_text(cx, &self.label);
            cx.turtle_align_y();
            self._bg_inst = Some(bg_inst);
            
            return Ok(())
        }
    }
    
    pub fn end_tab(&mut self, cx: &mut Cx) {
        if let Some(bg_inst) = self._bg_inst.take() {
            self._bg_area = self.bg.end_quad(cx, &bg_inst);
            self.animator.update_area_refs(cx, self._bg_area); // if our area changed, update animation
        }
    }
    
    pub fn draw_tab(&mut self, cx: &mut Cx) {
        if self.begin_tab(cx).is_err() {return};
        self.end_tab(cx);
    }
    
}