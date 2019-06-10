use render::*;

#[derive(Clone)]
pub struct ScrollBar {
    
    pub sb: Quad,
    pub bar_size: f32,
    pub min_handle_size: f32, //minimum size of the handle in pixels
    pub axis: Axis,
    pub animator: Animator,
    pub anim_over: Anim,
    pub anim_scrolling: Anim,
    pub use_vertical_finger_scroll: bool,
    pub _visible: bool,
    pub smoothing: Option<f32>,
    pub _sb_area: Area,
    pub _bar_side_margin: f32,
    pub _view_area: Area,
    pub _view_total: f32, // the total view area
    pub _view_visible: f32, // the visible view area
    pub _scroll_size: f32, // the size of the scrollbar
    pub _scroll_pos: f32, // scrolling position non normalised
    
    pub _scroll_target: f32,
    pub _scroll_delta: f32,
    
    pub _drag_point: Option<f32>, // the point in pixels where we are dragging
}

impl Style for ScrollBar {
    fn style(cx: &mut Cx) -> Self {
        let sb = Self::def_shader(cx);
        Self {
            bar_size: 12.0,
            min_handle_size: 30.0,
            smoothing: None,
            
            axis: Axis::Horizontal,
            animator: Animator::new(Anim::new(Play::Cut {duration: 0.5}, vec![
                Track::color("sb.color", Ease::Lin, vec![(1.0, color("#5"))])
            ])),
            anim_over: Anim::new(Play::Cut {duration: 0.05}, vec![
                Track::color("sb.color", Ease::Lin, vec![(1.0, color("#7"))])
            ]),
            anim_scrolling: Anim::new(Play::Cut {duration: 0.05}, vec![
                Track::color("sb.color", Ease::Lin, vec![(1.0, color("#9"))])
            ]),
            sb: Quad {
                shader: cx.add_shader(sb, "ScrollBar.sb"),
                ..Style::style(cx)
            },
            use_vertical_finger_scroll: false,
            _visible: false,
            
            _view_area: Area::Empty,
            _view_total: 0.0,
            _view_visible: 0.0,
            _bar_side_margin: 6.0,
            _scroll_size: 0.0,
            _scroll_pos: 0.0,
            
            _scroll_target: 0.0,
            _scroll_delta: 0.0,
            
            _drag_point: None,
            _sb_area: Area::Empty,
        }
    }
}


impl ScrollBar {
    pub fn def_shader(cx: &mut Cx) -> ShaderGen {
        let mut sg = Quad::def_quad_shader(cx);
        sg.add_ast(shader_ast!({
            
            let is_vertical: float<Instance>;
            
            let norm_handle: float<Instance>;
            let norm_scroll: float<Instance>;
            
            const border_radius: float = 1.5;
            
            fn vertex() -> vec4 {
                let clipped: vec2 = clamp(
                    geom * vec2(w, h) + vec2(x, y),
                    view_clip.xy,
                    view_clip.zw
                );
                pos = (clipped - vec2(x, y)) / vec2(w, h);
                return vec4(clipped, 0., 1.) * camera_projection;
            }
            
            fn pixel() -> vec4 {
                df_viewport(pos * vec2(w, h));
                if is_vertical > 0.5 {
                    df_box(1., h * norm_scroll, w * 0.5, h * norm_handle, border_radius);
                }
                else {
                    df_box(w * norm_scroll, 1., w * norm_handle, h * 0.5, border_radius);
                }
                return df_fill_keep(color);
            }
        }));
        sg
    }
    
    // reads back normalized scroll position info
    fn get_normalized_scroll_pos(&self) -> (f32, f32) {
        // computed handle size normalized
        let vy = self._view_visible / self._view_total;
        if !self._visible {
            return (0.0, 0.0);
        }
        let norm_handle = vy.max(self.min_handle_size / self._scroll_size);
        let norm_scroll = (1. - norm_handle) * ((self._scroll_pos / self._view_total) / (1. - vy));
        return (norm_scroll, norm_handle)
    }
    
    // sets the scroll pos from finger position
    fn set_scroll_pos_from_finger(&mut self, cx: &mut Cx, finger: f32) -> ScrollBarEvent {
        let vy = self._view_visible / self._view_total;
        let norm_handle = vy.max(self.min_handle_size / self._scroll_size);
        
        let new_scroll_pos = (
            (self._view_total * (1. - vy) * (finger / self._scroll_size)) / (1. - norm_handle)
        ).max(0.).min(self._view_total - self._view_visible);
        
        let changed = self._scroll_pos != new_scroll_pos;
        self._scroll_pos = new_scroll_pos;
        self._scroll_target = new_scroll_pos;
        if changed {
            self.update_shader_scroll_pos(cx);
            return self.make_scroll_event();
        }
        return ScrollBarEvent::None;
    }
    
    // writes the norm_scroll value into the shader
    fn update_shader_scroll_pos(&mut self, cx: &mut Cx) {
        let (norm_scroll, _) = self.get_normalized_scroll_pos();
        self._sb_area.write_float(cx, "norm_scroll", norm_scroll);
    }
    
    // turns scroll_pos into an event on this.event
    fn make_scroll_event(&mut self) -> ScrollBarEvent {
        ScrollBarEvent::Scroll {
            scroll_pos: self._scroll_pos,
            view_total: self._view_total,
            view_visible: self._view_visible
        }
    }
    
    fn move_towards_scroll_target(&mut self, cx: &mut Cx) -> bool {
        if self.smoothing.is_none() {
            return false;
        }
        if (self._scroll_target - self._scroll_pos).abs() < 0.01 {
            return false
        }
        if self._scroll_pos > self._scroll_target { // go back
            self._scroll_pos = self._scroll_pos + (self.smoothing.unwrap() * self._scroll_delta).min(-1.);
            if self._scroll_pos <= self._scroll_target { // hit the target
                self._scroll_pos = self._scroll_target;
                self.update_shader_scroll_pos(cx);
                return false;
            }
        }
        else { // go forward
            self._scroll_pos = self._scroll_pos + (self.smoothing.unwrap() * self._scroll_delta).max(1.);
            if self._scroll_pos > self._scroll_target { // hit the target
                self._scroll_pos = self._scroll_target;
                self.update_shader_scroll_pos(cx);
                return false;
            }
        }
        self.update_shader_scroll_pos(cx);
        true
    }
    
}


impl ScrollBarLike<ScrollBar> for ScrollBar {
    
    // public facing API
    
    fn get_scroll_pos(&self) -> f32 {
        return self._scroll_pos;
    }
    
    fn set_scroll_pos(&mut self, cx: &mut Cx, scroll_pos: f32) -> bool {
        // clamp scroll_pos to
        let scroll_pos = scroll_pos.min(self._view_total - self._view_visible).max(0.);
        
        if self._scroll_pos != scroll_pos {
            self._scroll_pos = scroll_pos;
            self._scroll_target = scroll_pos;
            self.update_shader_scroll_pos(cx);
            cx.next_frame(self._sb_area);
            return true
        };
        return false
    }
    
    fn get_scroll_target(&mut self) -> f32 {
        return self._scroll_target
    }
    
    fn set_scroll_view_total(&mut self, _cx: &mut Cx, view_total: f32) {
        self._view_total = view_total;
    }
    
    fn get_scroll_view_total(&self) -> f32 {
        return self._view_total;
    }
    
    fn set_scroll_target(&mut self, cx: &mut Cx, scroll_pos_target: f32) -> bool {
        // clamp scroll_pos to
        
        let new_target = scroll_pos_target.min(self._view_total - self._view_visible).max(0.);
        if self._scroll_target != new_target {
            self._scroll_target = new_target;
            self._scroll_delta = new_target - self._scroll_pos;
            cx.next_frame(self._sb_area);
            return true
        };
        return false
    }
    
    fn scroll_into_view(&mut self, cx: &mut Cx, pos: f32, size: f32) {
        if pos < self._scroll_pos { // scroll up
            let scroll_to = pos;
            if self.smoothing.is_none() {
                self.set_scroll_pos(cx, scroll_to);
            }
            else {
                self.set_scroll_target(cx, scroll_to);
            }
        }
        else if pos + size > self._scroll_pos + self._view_visible { // scroll down
            let scroll_to = (pos + size) - self._view_visible;
            if pos + size > self._view_total { // resize _view_total if need be
                self._view_total = pos + size;
            }
            if self.smoothing.is_none() {
                self.set_scroll_pos(cx, scroll_to);
            }
            else {
                self.set_scroll_target(cx, scroll_to);
            }
        }
    }
    
    fn handle_scroll_bar(&mut self, cx: &mut Cx, event: &mut Event) -> ScrollBarEvent {
        // lets check if our view-area gets a mouse-scroll.
        match event {
            Event::FingerScroll(fe) => {
                let rect = self._view_area.get_rect(cx, false);
                if rect.contains(fe.abs.x, fe.abs.y) { // handle mousewheel
                    // we should scroll in either x or y
                    let scroll = match self.axis {
                        Axis::Horizontal => if self.use_vertical_finger_scroll {fe.scroll.y}else {fe.scroll.x},
                        Axis::Vertical => fe.scroll.y
                    };
                    if !self.smoothing.is_none() {
                        let scroll_pos_target = self.get_scroll_target();
                        
                        self.set_scroll_target(cx, scroll_pos_target + scroll);
                        self.move_towards_scroll_target(cx); // take the first step now
                        return self.make_scroll_event();
                    }
                    else {
                        let scroll_pos = self.get_scroll_pos();
                        self.set_scroll_pos(cx, scroll_pos + scroll);
                        return self.make_scroll_event();
                    }
                }
            },
            
            _ => ()
        };
        if self._visible {
            match event.hits(cx, self._sb_area, HitOpt {no_scrolling: true, ..Default::default()}) {
                Event::Animate(ae) => {
                    self.animator.calc_write(cx, "sb.color", ae.time, self._sb_area);
                },
                Event::Frame(_ae) => {
                    if self.move_towards_scroll_target(cx) {
                        cx.next_frame(self._sb_area);
                    }
                    return self.make_scroll_event()
                },
                Event::FingerDown(fe) => {
                    self.animator.play_anim(cx, self.anim_scrolling.clone());
                    let rel = match self.axis {
                        Axis::Horizontal => fe.rel.x,
                        Axis::Vertical => fe.rel.y
                    };
                    let (norm_scroll, norm_handle) = self.get_normalized_scroll_pos();
                    let bar_start = norm_scroll * self._scroll_size;
                    let bar_size = norm_handle * self._scroll_size;
                    if rel < bar_start || rel > bar_start + bar_size { // clicked outside
                        self._drag_point = Some(bar_size * 0.5);
                        return self.set_scroll_pos_from_finger(cx, rel - self._drag_point.unwrap());
                    }
                    else { // clicked on
                        self._drag_point = Some(rel - bar_start); // store the drag delta
                    }
                },
                Event::FingerHover(fe) => {
                    if self._drag_point.is_none() {
                        match fe.hover_state {
                            HoverState::In => {
                                self.animator.play_anim(cx, self.anim_over.clone());
                            },
                            HoverState::Out => {
                                self.animator.play_anim(cx, self.animator.default.clone());
                            },
                            _ => ()
                        }
                    }
                },
                Event::FingerUp(fe) => {
                    self._drag_point = None;
                    if fe.is_over {
                        if !fe.is_touch {
                            self.animator.play_anim(cx, self.anim_over.clone());
                        }
                        else {
                            self.animator.play_anim(cx, self.animator.default.clone());
                        }
                    }
                    else {
                        self.animator.play_anim(cx, self.animator.default.clone());
                    }
                    return ScrollBarEvent::ScrollDone;
                },
                Event::FingerMove(fe) => {
                    // helper called by event code to scroll from a finger
                    if self._drag_point.is_none() {
                        // state should never occur.
                        //println!("Invalid state in scrollbar, fingerMove whilst drag_point is none")
                    }
                    else {
                        match self.axis {
                            Axis::Horizontal => {
                                return self.set_scroll_pos_from_finger(cx, fe.rel.x - self._drag_point.unwrap());
                            },
                            Axis::Vertical => {
                                return self.set_scroll_pos_from_finger(cx, fe.rel.y - self._drag_point.unwrap());
                            }
                        }
                    }
                },
                _ => ()
            };
        }
        
        ScrollBarEvent::None
    }
    
    fn draw_scroll_bar(&mut self, cx: &mut Cx, axis: Axis, view_area: Area, view_rect: Rect, view_total: Vec2) -> f32 {
        // pull the bg color from our animation system, uses 'default' value otherwise
        self.sb.color = self.animator.last_color("sb.color");
        self._sb_area = Area::Empty;
        self._view_area = view_area;
        self.axis = axis;
        
        match self.axis {
            Axis::Horizontal => {
                self._visible = view_total.x > view_rect.w + 0.1;
                self._scroll_size = if view_total.y > view_rect.h + 0.1 {
                    view_rect.w - self.bar_size
                }
                else {
                    view_rect.w
                } -self._bar_side_margin * 2.;
                self._view_total = view_total.x;
                self._view_visible = view_rect.w;
                
                if self._visible {
                    let sb_inst = self.sb.draw_quad(
                        cx,
                        Rect {
                            x: self._bar_side_margin,
                            y: view_rect.h - self.bar_size,
                            w: self._scroll_size,
                            h: self.bar_size,
                        }
                    );
                    //is_vertical
                    let (norm_scroll, norm_handle) = self.get_normalized_scroll_pos();
                    sb_inst.push_float(cx, 0.0);
                    sb_inst.push_float(cx, norm_handle);
                    sb_inst.push_float(cx, norm_scroll);
                    self._sb_area = sb_inst.into_area();
                }
            },
            Axis::Vertical => {
                // compute if we need a horizontal one
                self._visible = view_total.y > view_rect.h + 0.1;
                self._scroll_size = if view_total.x > view_rect.w + 0.1 {
                    view_rect.h - self.bar_size
                }
                else {
                    view_rect.h
                } -self._bar_side_margin * 2.;
                self._view_total = view_total.y;
                self._view_visible = view_rect.h;
                if self._visible {
                    let sb_inst = self.sb.draw_quad(
                        cx,
                        Rect {
                            x: view_rect.w - self.bar_size,
                            y: self._bar_side_margin,
                            w: self.bar_size,
                            h: self._scroll_size
                        }
                    );
                    //is_vertical
                    let (norm_scroll, norm_handle) = self.get_normalized_scroll_pos();
                    sb_inst.push_float(cx, 1.0);
                    sb_inst.push_float(cx, norm_handle);
                    sb_inst.push_float(cx, norm_scroll);
                    self._sb_area = sb_inst.into_area();
                }
            }
        }
        
        // push the var added to the sb shader
        if self._visible {
            self.animator.update_area_refs(cx, self._sb_area); // if our area changed, update animation
        }
        
        // see if we need to clamp
        let clamped_pos = self._scroll_pos.min(self._view_total - self._view_visible).max(0.);
        if clamped_pos != self._scroll_pos {
            self._scroll_pos = clamped_pos;
            self._scroll_target = clamped_pos;
            // ok so this means we 'scrolled' this can give a problem for virtual viewport widgets
            cx.next_frame(self._sb_area);
        }
        
        self._scroll_pos
    }
}
