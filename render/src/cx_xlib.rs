use crate::cx::*;
use libc;
use libc::timeval;
use std::collections::{HashMap, VecDeque};
use std::ffi::CString;
use std::ffi::CStr;
use std::sync::Mutex;
//use std::fs::File;
//use std::io::Write;
//use std::os::unix::io::FromRawFd;
use std::mem;
use std::os::raw::{c_char, c_int, c_uint, c_ulong, c_long};
use std::ptr;
use time::precise_time_ns;
use x11_dl::xlib;
use x11_dl::xlib::{Display, XVisualInfo, Xlib};
use x11_dl::keysym;
use x11_dl::xcursor::Xcursor;

static mut GLOBAL_XLIB_APP: *mut XlibApp = 0 as *mut _;

pub struct XlibApp {
    pub xlib: Xlib,
    pub xcursor: Xcursor,
    pub display: *mut Display,
    
    pub display_fd: c_int,
    pub signal_fd: c_int,
    pub window_map: HashMap<c_ulong, *mut XlibWindow>,
    pub time_start: u64,
    pub last_scroll_time: f64,
    pub event_callback: Option<*mut FnMut(&mut XlibApp, &mut Vec<Event>) -> bool>,
    pub event_recur_block: bool,
    pub event_loop_running: bool,
    pub timers: VecDeque<XlibTimer>,
    pub free_timers: Vec<usize>,
    pub signals: Mutex<Vec<Event>>,
    pub loop_block: bool,
    pub current_cursor: MouseCursor,
}

#[derive(Clone)]
pub struct XlibWindow {
    pub window: Option<c_ulong>,
    pub attributes: Option<xlib::XSetWindowAttributes>,
    pub visual_info: Option<XVisualInfo>,
    pub child_windows: Vec<XlibChildWindow>,
    
    pub last_nc_mode: XlibNcMode,
    pub window_id: usize,
    pub xlib_app: *mut XlibApp,
    pub last_window_geom: WindowGeom,
    pub time_start: u64,
    
    pub ime_spot: Vec2,
    pub current_cursor: MouseCursor,
    pub last_mouse_pos: Vec2,
    pub fingers_down: Vec<bool>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum XlibNcMode {
    Client,
    Caption,
    TopLeft,
    Top,
    TopRight,
    Right,
    BottomRight,
    Bottom,
    BottomLeft,
    Left,
}

#[derive(Clone)]
pub struct XlibChildWindow {
    pub window: c_ulong,
    visible: bool,
    x: i32,
    y: i32,
    w: u32,
    h: u32
}

#[derive(Clone, Copy, Debug)]
pub struct XlibTimer {
    id: u64,
    timeout: f64,
    repeats: bool,
    delta_timeout: f64,
}

#[derive(Clone)]
pub struct XlibSignal {
    pub signal_id: u64,
    pub value: u64
}

impl XlibApp {
    pub fn new() -> XlibApp {
        unsafe {
            let xlib = Xlib::open().unwrap();
            let xcursor = Xcursor::open().unwrap();
            let display = (xlib.XOpenDisplay)(ptr::null());
            let display_fd = (xlib.XConnectionNumber)(display);
            let signal_fd = 0i32; //libc::pipe();
            XlibApp {
                xlib,
                xcursor,
                display,
                display_fd,
                signal_fd,
                last_scroll_time: 0.0,
                window_map: HashMap::new(),
                signals: Mutex::new(Vec::new()),
                time_start: precise_time_ns(),
                event_callback: None,
                event_recur_block: false,
                event_loop_running: true,
                loop_block: false,
                timers: VecDeque::new(),
                free_timers: Vec::new(),
                current_cursor: MouseCursor::Default
            }
        }
    }
    
    pub fn init(&mut self) {
        unsafe {
            //unsafe {
            (self.xlib.XrmInitialize)();
            //}
            GLOBAL_XLIB_APP = self;
        }
    }
    
    pub fn event_loop<F>(&mut self, mut event_handler: F)
    where F: FnMut(&mut XlibApp, &mut Vec<Event>) -> bool,
    {
        unsafe {
            self.event_callback = Some(
                &mut event_handler as *const FnMut(&mut XlibApp, &mut Vec<Event>) -> bool
                as *mut FnMut(&mut XlibApp, &mut Vec<Event>) -> bool
            );
            
            self.do_callback(&mut vec![
                Event::Paint,
            ]);
            
            // Record the current time.
            let mut select_time = self.time_now();
            
            while self.event_loop_running {
                if self.loop_block {
                    let mut fds = mem::uninitialized();
                    libc::FD_ZERO(&mut fds);
                    libc::FD_SET(self.display_fd, &mut fds);
                    // If there are any timers, we set the timeout for select to the `delta_timeout`
                    // of the first timer that should be fired. Otherwise, we set the timeout to
                    // None, so that select will block indefinitely.
                    let timeout = if let Some(timer) = self.timers.front() {
                        // println!("Select wait {}",(timer.delta_timeout.fract() * 1000000.0) as i64);
                        Some(timeval {
                            // `tv_sec` is in seconds, so take the integer part of `delta_timeout`
                            tv_sec: timer.delta_timeout.trunc() as libc::time_t,
                            // `tv_usec` is in microseconds, so take the fractional part of
                            // `delta_timeout` 1000000.0.
                            tv_usec: (timer.delta_timeout.fract() * 1000000.0) as libc::time_t,
                        })
                    }
                    else {
                        None
                    };
                    let _nfds = libc::select(
                        self.display_fd + 1,
                        &mut fds,
                        ptr::null_mut(),
                        ptr::null_mut(),
                        if let Some(mut timeout) = timeout {&mut timeout} else {ptr::null_mut()}
                    );
                }
                // Update the current time, and compute the amount of time that elapsed since we
                // last recorded the current time.
                let last_select_time = select_time;
                select_time = self.time_now();
                let mut select_time_used = select_time - last_select_time;
                
                while let Some(timer) = self.timers.front_mut() {
                    // If the amount of time that elapsed is less than `delta_timeout` for the
                    // next timer, then no more timers need to be fired.
                    if select_time_used < timer.delta_timeout {
                        timer.delta_timeout -= select_time_used;
                        break;
                    }
                    
                    let timer = *self.timers.front().unwrap();
                    select_time_used -= timer.delta_timeout;
                    
                    // Stop the timer to remove it from the list.
                    self.stop_timer(timer.id);
                    // If the timer is repeating, simply start it again.
                    if timer.repeats {
                        self.start_timer(timer.id, timer.timeout, timer.repeats);
                    }
                    // Fire the timer, and allow the callback to cancel the repeat
                    self.do_callback(&mut vec![
                        Event::Timer(TimerEvent {timer_id: timer.id})
                    ]);
                }
                
                while (self.xlib.XPending)(self.display) != 0 {
                    let mut event = mem::uninitialized();
                    (self.xlib.XNextEvent)(self.display, &mut event);
                    match event.type_ {
                        xlib::ConfigureNotify => {
                            let cfg = event.configure;
                            if let Some(window_ptr) = self.window_map.get(&cfg.window) {
                                let window = &mut (**window_ptr);
                                if cfg.window == window.window.unwrap() {
                                    window.send_change_event();
                                }
                            }
                        },
                        xlib::EnterNotify => {
                            
                        },
                        xlib::LeaveNotify => {
                            let crossing = event.crossing;
                            if crossing.detail == 4 {
                                if let Some(window_ptr) = self.window_map.get(&crossing.window) {
                                    let window = &mut (**window_ptr);
                                    window.do_callback(&mut vec![Event::FingerHover(FingerHoverEvent {
                                        window_id: window.window_id,
                                        any_down: false,
                                        abs: window.last_mouse_pos,
                                        rel: window.last_mouse_pos,
                                        rect: Rect::zero(),
                                        handled: false,
                                        hover_state: HoverState::Out,
                                        modifiers: KeyModifiers::default(),
                                        time: window.time_now()
                                    })]);
                                }
                            }
                        },
                        xlib::MotionNotify => { // mousemove
                            let motion = event.motion;
                            if let Some(window_ptr) = self.window_map.get(&motion.window) {
                                let window = &mut (**window_ptr);
                                let mut x = motion.x;
                                let mut y = motion.y;
                                if motion.window != window.window.unwrap() {
                                    // find the right child
                                    for child in &window.child_windows {
                                        if child.window == motion.window {
                                            x += child.x;
                                            y += child.y;
                                            break
                                        }
                                    }
                                }
                                
                                let pos = Vec2 {x: x as f32 / window.last_window_geom.dpi_factor, y: y as f32 / window.last_window_geom.dpi_factor};
                                
                                // query window for chrome
                                let mut drag_query_events = vec![
                                    Event::WindowDragQuery(WindowDragQueryEvent {
                                        window_id: window.window_id,
                                        abs: window.last_mouse_pos,
                                        response: WindowDragQueryResponse::NoAnswer
                                    })
                                ];
                                window.do_callback(&mut drag_query_events);
                                // otherwise lets check if we are hover the window edge to resize the window
                                //println!("{} {}", window.last_window_geom.inner_size.x, pos.x);
                                window.send_finger_hover_and_move(pos, KeyModifiers::default());
                                let window_size = window.last_window_geom.inner_size;
                                if pos.x >= 0.0 && pos.x < 10.0 && pos.y >= 0.0 && pos.y < 10.0 {
                                    window.last_nc_mode = XlibNcMode::TopLeft;
                                    window.do_callback(&mut vec![Event::WindowSetHoverCursor(MouseCursor::NwResize)]);
                                }
                                else if pos.x >= 0.0 && pos.x < 10.0 && pos.y >= window_size.y - 10.0 {
                                    window.last_nc_mode = XlibNcMode::BottomLeft;
                                    window.do_callback(&mut vec![Event::WindowSetHoverCursor(MouseCursor::SwResize)]);
                                }
                                else if pos.x >= 0.0 && pos.x < 5.0 {
                                    window.last_nc_mode = XlibNcMode::Left;
                                    window.do_callback(&mut vec![Event::WindowSetHoverCursor(MouseCursor::WResize)]);
                                }
                                else if pos.x >= window_size.x - 10.0 && pos.y >= 0.0 && pos.y < 10.0 {
                                    window.last_nc_mode = XlibNcMode::TopRight;
                                    window.do_callback(&mut vec![Event::WindowSetHoverCursor(MouseCursor::NeResize)]);
                                }
                                else if pos.x >= window_size.x - 10.0 && pos.y >= window_size.y - 10.0 {
                                    window.last_nc_mode = XlibNcMode::BottomRight;
                                    window.do_callback(&mut vec![Event::WindowSetHoverCursor(MouseCursor::SeResize)]);
                                }
                                else if pos.x >= window_size.x - 5.0 {
                                    window.last_nc_mode = XlibNcMode::Right;
                                    window.do_callback(&mut vec![Event::WindowSetHoverCursor(MouseCursor::EResize)]);
                                }
                                else if pos.y <= 5.0 {
                                    window.last_nc_mode = XlibNcMode::Top;
                                    window.do_callback(&mut vec![Event::WindowSetHoverCursor(MouseCursor::NResize)]);
                                }
                                else if pos.y > window_size.y - 5.0 {
                                    window.last_nc_mode = XlibNcMode::Bottom;
                                    window.do_callback(&mut vec![Event::WindowSetHoverCursor(MouseCursor::SResize)]);
                                }
                                else {
                                    match &drag_query_events[0] {
                                        Event::WindowDragQuery(wd) => match &wd.response {
                                            WindowDragQueryResponse::Caption => {
                                                window.last_nc_mode = XlibNcMode::Caption;
                                            },
                                            _ => {
                                                window.last_nc_mode = XlibNcMode::Client;
                                            }
                                        },
                                        _ => ()
                                    }
                                }
                            }
                        },
                        xlib::ButtonPress => { // mouse down
                            let button = event.button;
                            if let Some(window_ptr) = self.window_map.get(&button.window) {
                                let window = &mut (**window_ptr);
                                (self.xlib.XSetInputFocus)(self.display, window.window.unwrap(), xlib::RevertToNone, xlib::CurrentTime);
                                // its a mousewheel
                                if button.button >= 4 && button.button <= 7 {
                                    let last_scroll_time = self.last_scroll_time;
                                    self.last_scroll_time = self.time_now();
                                    // completely arbitrary scroll acceleration curve.
                                    let speed = 1200.0 * (0.2 - 2. * (self.last_scroll_time - last_scroll_time)).max(0.01);
                                    self.do_callback(&mut vec![Event::FingerScroll(FingerScrollEvent {
                                        window_id: window.window_id,
                                        scroll: Vec2 {
                                            x: 0.0,
                                            y: if button.button == 4 {-speed as f32} else {speed as f32}
                                        },
                                        abs: window.last_mouse_pos,
                                        rel: window.last_mouse_pos,
                                        rect: Rect::zero(),
                                        is_wheel: true,
                                        modifiers: self.xkeystate_to_modifiers(button.state),
                                        handled: false,
                                        time: self.last_scroll_time
                                    })])
                                }
                                else {
                                    println!("HERE!{:?}", window.last_nc_mode);
                                    // lets check if last_nc_mouse_pos is something we need to do
                                    if window.last_nc_mode == XlibNcMode::Client {
                                        window.send_finger_down(button.button as usize, self.xkeystate_to_modifiers(button.state))
                                    }
                                    else { // start some kind of drag
                                        // tell the window manager to start dragging
                                        let default_screen = (self.xlib.XDefaultScreen)(self.display);
                                        
                                        // The root window of the default screen
                                        let root_window = (self.xlib.XRootWindow)(self.display, default_screen);
                                        (self.xlib.XUngrabPointer)(self.display, 0);
                                        (self.xlib.XFlush)(self.display);
                                        let mut xclient = xlib::XClientMessageEvent {
                                            type_: xlib::ClientMessage,
                                            serial: 0,
                                            send_event: 0,
                                            display: self.display,
                                            window: window.window.unwrap(),
                                            message_type: (self.xlib.XInternAtom)(self.display, CString::new("_NET_WM_MOVERESIZE").unwrap().as_ptr(), 0),
                                            format: 32,
                                            data: {
                                                let mut msg = xlib::ClientMessageData::new();
                                                msg.set_long(0, button.x_root as c_long);
                                                msg.set_long(1, button.y_root as c_long);
                                                msg.set_long(2, match window.last_nc_mode {
                                                    XlibNcMode::TopLeft => _NET_WM_MOVERESIZE_SIZE_TOPLEFT,
                                                    XlibNcMode::Top => _NET_WM_MOVERESIZE_SIZE_TOP,
                                                    XlibNcMode::TopRight => _NET_WM_MOVERESIZE_SIZE_TOPRIGHT,
                                                    XlibNcMode::Right => _NET_WM_MOVERESIZE_SIZE_RIGHT,
                                                    XlibNcMode::BottomRight => _NET_WM_MOVERESIZE_SIZE_BOTTOMRIGHT,
                                                    XlibNcMode::Bottom => _NET_WM_MOVERESIZE_SIZE_BOTTOM,
                                                    XlibNcMode::BottomLeft => _NET_WM_MOVERESIZE_SIZE_BOTTOMLEFT,
                                                    XlibNcMode::Left => _NET_WM_MOVERESIZE_SIZE_LEFT,
                                                    _ => _NET_WM_MOVERESIZE_MOVE,
                                                });
                                                msg
                                            }
                                        };
                                        (self.xlib.XSendEvent)(self.display, root_window, 0, xlib::SubstructureRedirectMask | xlib::SubstructureNotifyMask, &mut xclient as *mut _ as *mut xlib::XEvent);
                                    }
                                }
                            }
                        },
                        xlib::ButtonRelease => { // mouse up
                            let button = event.button;
                            if let Some(window_ptr) = self.window_map.get(&button.window) {
                                let window = &mut (**window_ptr);
                                window.send_finger_up(button.button as usize, self.xkeystate_to_modifiers(button.state))
                            }
                        },
                        xlib::KeyPress => {
                            self.do_callback(&mut vec![Event::KeyDown(KeyEvent {
                                key_code: self.xkeyevent_to_keycode(&mut event.key),
                                is_repeat: false,
                                modifiers: self.xkeystate_to_modifiers(event.key.state),
                                time: self.time_now()
                            })]);
                        },
                        xlib::KeyRelease => {
                            self.do_callback(&mut vec![Event::KeyUp(KeyEvent {
                                key_code: self.xkeyevent_to_keycode(&mut event.key),
                                is_repeat: false,
                                modifiers: self.xkeystate_to_modifiers(event.key.state),
                                time: self.time_now()
                            })]);
                        },
                        xlib::ClientMessage => {
                            /*
                            if event.client_message.data.get_long(0) as u64 == wm_delete_message {
                                self.event_loop_running = false;
                            }*/
                        },
                        xlib::Expose => {
                            /*
                            (glx.glXMakeCurrent)(display, window, context);
                            gl::ClearColor(1.0, 0.0, 0.0, 1.0);
                            gl::Clear(gl::COLOR_BUFFER_BIT);
                            (glx.glXSwapBuffers)(display, window);
                            */
                        },
                        _ => {}
                    }
                }
                // process all signals in the queue
                let mut proc_signals = if let Ok(mut signals) = self.signals.lock() {
                    let sigs = signals.clone();
                    signals.truncate(0);
                    sigs
                }
                else {
                    Vec::new()
                };
                if proc_signals.len() > 0 {
                    self.do_callback(&mut proc_signals);
                }
                
                self.do_callback(&mut vec![
                    Event::Paint,
                ]);
            }
            
            self.event_callback = None;
        }
    }
    
    pub fn do_callback(&mut self, events: &mut Vec<Event>) {
        unsafe {
            if self.event_callback.is_none() || self.event_recur_block {
                return
            };
            self.event_recur_block = true;
            let callback = self.event_callback.unwrap();
            self.loop_block = (*callback)(self, events);
            self.event_recur_block = false;
        }
    }
    
    pub fn start_timer(&mut self, id: u64, timeout: f64, repeats: bool) {
        //println!("STARTING TIMER {:?} {:?} {:?}", id, timeout, repeats);
        
        // Timers are stored in an ordered list. Each timer stores the amount of time between
        // when its predecessor in the list should fire and when the timer itself should fire
        // in `delta_timeout`.
        
        // Since we are starting a new timer, our first step is to find where in the list this
        // new timer should be inserted. `delta_timeout` is initially set to `timeout`. As we move
        // through the list, we subtract the `delta_timeout` of the timers preceding the new timer
        // in the list. Once this subtraction would cause an overflow, we have found the correct
        // position in the list. The timer should fire after the one preceding it in the list, and
        // before the one succeeding it in the list. Moreover `delta_timeout` is now set to the
        // correct value.
        let mut delta_timeout = timeout;
        let index = self.timers.iter().position( | timer | {
            if delta_timeout < timer.delta_timeout {
                return true;
            }
            delta_timeout -= timer.delta_timeout;
            false
        }).unwrap_or(self.timers.len());
        
        // Insert the timer in the list.
        //
        // We also store the original `timeout` with each timer. This is necessary if the timer is
        // repeatable and we want to restart it later on.
        self.timers.insert(
            index,
            XlibTimer {
                id,
                timeout,
                repeats,
                delta_timeout,
            },
        );
        
        // The timer succeeding the newly inserted timer now has a new timer preceding it, so we
        // need to adjust its `delta_timeout`.
        //
        // Note that by construction, `timer.delta_timeout < delta_timeout`. Otherwise, the newly
        // inserted timer would have been inserted *after* the timer succeeding it, not before it.
        if index < self.timers.len() - 1 {
            let timer = &mut self.timers[index + 1];
            // This computation should never underflow (see above)
            timer.delta_timeout -= delta_timeout;
        }
    }
    
    pub fn stop_timer(&mut self, id: u64) {
        //println!("STOPPING TIMER {:?}", id);
        
        // Since we are stopping an existing timer, our first step is to find where in the list this
        // timer should be removed.
        let index = if let Some(index) = self.timers.iter().position( | timer | timer.id == id) {
            index
        } else {
            return;
        };
        
        // Remove the timer from the list.
        let delta_timeout = self.timers.remove(index).unwrap().delta_timeout;
        
        // The timer succeeding the removed timer now has a different timer preceding it, so we need
        // to adjust its `delta timeout`.
        if index < self.timers.len() {
            self.timers[index].delta_timeout += delta_timeout;
        }
    }
    
    pub fn post_signal(signal_id: usize, value: usize) {
        unsafe {
            if let Ok(mut signals) = (*GLOBAL_XLIB_APP).signals.lock() {
                signals.push(Event::Signal(SignalEvent {signal_id, value}));
                //let mut f = unsafe { File::from_raw_fd((*GLOBAL_XLIB_APP).display_fd) };
                //let _ = write!(&mut f, "\0");
                // !TODO unblock the select!
            }
        }
    }
    
    pub fn terminate_event_loop(&mut self) {
        // maybe need to do more here
        self.event_loop_running = false;
        
        unsafe {(self.xlib.XCloseDisplay)(self.display)};
    }
    
    pub fn time_now(&self) -> f64 {
        let time_now = precise_time_ns();
        (time_now - self.time_start) as f64 / 1_000_000_000.0
    }
    
    pub fn load_first_cursor(&self, names: &[&[u8]]) -> Option<c_ulong> {
        unsafe {
            for name in names {
                let cursor = (self.xcursor.XcursorLibraryLoadCursor)(
                    self.display,
                    name.as_ptr() as *const c_char,
                );
                if cursor != 0 {
                    return Some(cursor)
                }
            }
        }
        return None
    }
    
    pub fn set_mouse_cursor(&mut self, cursor: MouseCursor) {
        if self.current_cursor != cursor {
            self.current_cursor = cursor.clone();
            let x11_cursor = match cursor {
                MouseCursor::Hidden => {
                    return;
                },
                MouseCursor::EResize => self.load_first_cursor(&[b"right_side\0"]),
                MouseCursor::NResize => self.load_first_cursor(&[b"top_side\0"]),
                MouseCursor::NeResize => self.load_first_cursor(&[b"top_right_corner\0"]),
                MouseCursor::NwResize => self.load_first_cursor(&[b"top_left_corner\0"]),
                MouseCursor::SResize => self.load_first_cursor(&[b"bottom_side\0"]),
                MouseCursor::SeResize => self.load_first_cursor(&[b"bottom_right_corner\0"]),
                MouseCursor::SwResize => self.load_first_cursor(&[b"bottom_left_corner\0"]),
                MouseCursor::WResize => self.load_first_cursor(&[b"left_side\0"]),
                
                MouseCursor::Default => self.load_first_cursor(&[b"left_ptr\0"]),
                MouseCursor::Crosshair => self.load_first_cursor(&[b"crosshair"]),
                MouseCursor::Hand => self.load_first_cursor(&[b"hand2\0", b"hand1\0"]),
                MouseCursor::Arrow => self.load_first_cursor(&[b"arrow\0"]),
                MouseCursor::Move => self.load_first_cursor(&[b"move\0"]),
                MouseCursor::NotAllowed => self.load_first_cursor(&[b"crossed_circle\0"]),
                MouseCursor::Text => self.load_first_cursor(&[b"text\0", b"xterm\0"]),
                MouseCursor::Wait => self.load_first_cursor(&[b"watch\0"]),
                MouseCursor::Help => self.load_first_cursor(&[b"question_arrow\0"]),
                MouseCursor::NsResize => self.load_first_cursor(&[b"v_double_arrow\0"]),
                MouseCursor::NeswResize => self.load_first_cursor(&[b"fd_double_arrow\0", b"size_fdiag\0"]),
                MouseCursor::EwResize => self.load_first_cursor(&[b"h_double_arrow\0"]),
                MouseCursor::NwseResize => self.load_first_cursor(&[b"bd_double_arrow\0", b"size_bdiag\0"]),
                MouseCursor::ColResize => self.load_first_cursor(&[b"split_h\0", b"h_double_arrow\0"]),
                MouseCursor::RowResize => self.load_first_cursor(&[b"split_v\0", b"v_double_arrow\0"]),
            };
            if let Some(x11_cursor) = x11_cursor {
                unsafe {
                    for (k, _v) in &self.window_map {
                        (self.xlib.XDefineCursor)(self.display, *k, x11_cursor);
                    }
                    (self.xlib.XFreeCursor)(self.display, x11_cursor);
                }
            }
        }
    }
    
    fn xkeystate_to_modifiers(&self, state: c_uint) -> KeyModifiers {
        KeyModifiers {
            alt: state & xlib::Mod1Mask != 0,
            shift: state & xlib::ShiftMask != 0,
            control: state & xlib::ControlMask != 0,
            logo: state & xlib::Mod4Mask != 0,
        }
    }
    
    fn xkeyevent_to_keycode(&self, key_event: &mut xlib::XKeyEvent) -> KeyCode {
        let mut keysym = 0;
        unsafe {
            (self.xlib.XLookupString)(
                key_event,
                ptr::null_mut(),
                0,
                &mut keysym,
                ptr::null_mut(),
            );
        }
        match keysym as u32 {
            keysym::XK_a => KeyCode::KeyA,
            keysym::XK_A => KeyCode::KeyA,
            keysym::XK_b => KeyCode::KeyB,
            keysym::XK_B => KeyCode::KeyB,
            keysym::XK_c => KeyCode::KeyC,
            keysym::XK_C => KeyCode::KeyC,
            keysym::XK_d => KeyCode::KeyD,
            keysym::XK_D => KeyCode::KeyD,
            keysym::XK_e => KeyCode::KeyE,
            keysym::XK_E => KeyCode::KeyE,
            keysym::XK_f => KeyCode::KeyF,
            keysym::XK_F => KeyCode::KeyF,
            keysym::XK_g => KeyCode::KeyG,
            keysym::XK_G => KeyCode::KeyG,
            keysym::XK_h => KeyCode::KeyH,
            keysym::XK_H => KeyCode::KeyH,
            keysym::XK_i => KeyCode::KeyI,
            keysym::XK_I => KeyCode::KeyI,
            keysym::XK_j => KeyCode::KeyJ,
            keysym::XK_J => KeyCode::KeyJ,
            keysym::XK_k => KeyCode::KeyK,
            keysym::XK_K => KeyCode::KeyK,
            keysym::XK_l => KeyCode::KeyL,
            keysym::XK_L => KeyCode::KeyL,
            keysym::XK_m => KeyCode::KeyM,
            keysym::XK_M => KeyCode::KeyM,
            keysym::XK_n => KeyCode::KeyN,
            keysym::XK_N => KeyCode::KeyN,
            keysym::XK_o => KeyCode::KeyO,
            keysym::XK_O => KeyCode::KeyO,
            keysym::XK_p => KeyCode::KeyP,
            keysym::XK_P => KeyCode::KeyP,
            keysym::XK_q => KeyCode::KeyQ,
            keysym::XK_Q => KeyCode::KeyQ,
            keysym::XK_r => KeyCode::KeyR,
            keysym::XK_R => KeyCode::KeyR,
            keysym::XK_s => KeyCode::KeyS,
            keysym::XK_S => KeyCode::KeyS,
            keysym::XK_t => KeyCode::KeyT,
            keysym::XK_T => KeyCode::KeyT,
            keysym::XK_u => KeyCode::KeyU,
            keysym::XK_U => KeyCode::KeyU,
            keysym::XK_v => KeyCode::KeyV,
            keysym::XK_V => KeyCode::KeyV,
            keysym::XK_w => KeyCode::KeyW,
            keysym::XK_W => KeyCode::KeyW,
            keysym::XK_x => KeyCode::KeyX,
            keysym::XK_X => KeyCode::KeyX,
            keysym::XK_y => KeyCode::KeyY,
            keysym::XK_Y => KeyCode::KeyY,
            keysym::XK_z => KeyCode::KeyZ,
            keysym::XK_Z => KeyCode::KeyZ,
            
            keysym::XK_0 => KeyCode::Key0,
            keysym::XK_1 => KeyCode::Key1,
            keysym::XK_2 => KeyCode::Key2,
            keysym::XK_3 => KeyCode::Key3,
            keysym::XK_4 => KeyCode::Key4,
            keysym::XK_5 => KeyCode::Key5,
            keysym::XK_6 => KeyCode::Key6,
            keysym::XK_7 => KeyCode::Key7,
            keysym::XK_8 => KeyCode::Key8,
            keysym::XK_9 => KeyCode::Key9,
            
            keysym::XK_Alt_L => KeyCode::Alt,
            keysym::XK_Alt_R => KeyCode::Alt,
            keysym::XK_Meta_L => KeyCode::Logo,
            keysym::XK_Meta_R => KeyCode::Logo,
            keysym::XK_Shift_L => KeyCode::Shift,
            keysym::XK_Shift_R => KeyCode::Shift,
            keysym::XK_Control_L => KeyCode::Control,
            keysym::XK_Control_R => KeyCode::Control,
            
            keysym::XK_equal => KeyCode::Equals,
            keysym::XK_minus => KeyCode::Minus,
            keysym::XK_bracketright => KeyCode::RBracket,
            keysym::XK_bracketleft => KeyCode::LBracket,
            keysym::XK_Return => KeyCode::Return,
            keysym::XK_grave => KeyCode::Backtick,
            keysym::XK_semicolon => KeyCode::Semicolon,
            keysym::XK_backslash => KeyCode::Backslash,
            keysym::XK_comma => KeyCode::Comma,
            keysym::XK_slash => KeyCode::Slash,
            keysym::XK_period => KeyCode::Period,
            keysym::XK_Tab => KeyCode::Tab,
            keysym::XK_space => KeyCode::Space,
            keysym::XK_BackSpace => KeyCode::Backspace,
            keysym::XK_Escape => KeyCode::Escape,
            keysym::XK_Caps_Lock => KeyCode::Capslock,
            keysym::XK_KP_Decimal => KeyCode::NumpadDecimal,
            keysym::XK_KP_Multiply => KeyCode::NumpadMultiply,
            keysym::XK_KP_Add => KeyCode::NumpadAdd,
            keysym::XK_Num_Lock => KeyCode::Numlock,
            keysym::XK_KP_Divide => KeyCode::NumpadDivide,
            keysym::XK_KP_Enter => KeyCode::NumpadEnter,
            keysym::XK_KP_Subtract => KeyCode::NumpadSubtract,
            //keysim::XK_9 => KeyCode::NumpadEquals,
            keysym::XK_KP_0 => KeyCode::Numpad0,
            keysym::XK_KP_1 => KeyCode::Numpad1,
            keysym::XK_KP_2 => KeyCode::Numpad2,
            keysym::XK_KP_3 => KeyCode::Numpad3,
            keysym::XK_KP_4 => KeyCode::Numpad4,
            keysym::XK_KP_5 => KeyCode::Numpad5,
            keysym::XK_KP_6 => KeyCode::Numpad6,
            keysym::XK_KP_7 => KeyCode::Numpad7,
            keysym::XK_KP_8 => KeyCode::Numpad8,
            keysym::XK_KP_9 => KeyCode::Numpad9,
            
            keysym::XK_F1 => KeyCode::F1,
            keysym::XK_F2 => KeyCode::F2,
            keysym::XK_F3 => KeyCode::F3,
            keysym::XK_F4 => KeyCode::F4,
            keysym::XK_F5 => KeyCode::F5,
            keysym::XK_F6 => KeyCode::F6,
            keysym::XK_F7 => KeyCode::F7,
            keysym::XK_F8 => KeyCode::F8,
            keysym::XK_F9 => KeyCode::F9,
            keysym::XK_F10 => KeyCode::F10,
            keysym::XK_F11 => KeyCode::F11,
            keysym::XK_F12 => KeyCode::F12,
            
            keysym::XK_Print => KeyCode::PrintScreen,
            keysym::XK_Home => KeyCode::Home,
            keysym::XK_Page_Up => KeyCode::PageUp,
            keysym::XK_Delete => KeyCode::Delete,
            keysym::XK_End => KeyCode::End,
            keysym::XK_Page_Down => KeyCode::PageDown,
            keysym::XK_Left => KeyCode::ArrowLeft,
            keysym::XK_Right => KeyCode::ArrowRight,
            keysym::XK_Down => KeyCode::ArrowDown,
            keysym::XK_Up => KeyCode::ArrowUp,
            _ => KeyCode::Unknown,
        }
    }
}


impl XlibWindow {
    
    pub fn new(xlib_app: &mut XlibApp, window_id: usize) -> XlibWindow {
        let mut fingers_down = Vec::new();
        fingers_down.resize(NUM_FINGERS, false);
        
        XlibWindow {
            window: None,
            attributes: None,
            visual_info: None,
            child_windows: Vec::new(),
            window_id: window_id,
            xlib_app: xlib_app,
            last_window_geom: WindowGeom::default(),
            time_start: xlib_app.time_start,
            last_nc_mode: XlibNcMode::Client,
            ime_spot: Vec2::zero(),
            current_cursor: MouseCursor::Default,
            last_mouse_pos: Vec2::zero(),
            fingers_down: fingers_down,
        }
    }
    
    pub fn init(&mut self, _title: &str, size: Vec2, position: Option<Vec2>, visual_info: *const XVisualInfo) {
        unsafe {
            let xlib = &(*self.xlib_app).xlib;
            let display = (*self.xlib_app).display;
            
            // The default screen of the display
            let default_screen = (xlib.XDefaultScreen)(display);
            
            // The root window of the default screen
            let root_window = (xlib.XRootWindow)(display, default_screen);
            
            let mut attributes = mem::zeroed::<xlib::XSetWindowAttributes>();
            
            attributes.border_pixel = 0;
            //attributes.override_redirect = 1;
            attributes.colormap =
            (xlib.XCreateColormap)(display, root_window, (*visual_info).visual, xlib::AllocNone);
            attributes.event_mask = xlib::ExposureMask
                | xlib::StructureNotifyMask
                | xlib::ButtonMotionMask
                | xlib::PointerMotionMask
                | xlib::ButtonPressMask
                | xlib::ButtonReleaseMask
                | xlib::KeyPressMask
                | xlib::KeyReleaseMask
                | xlib::VisibilityChangeMask
                | xlib::FocusChangeMask
                | xlib::EnterWindowMask
                | xlib::LeaveWindowMask;
            
            
            let dpi_factor = self.get_dpi_factor();
            // Create a window
            let window = (xlib.XCreateWindow)(
                display,
                root_window,
                if position.is_some() {position.unwrap().x}else {150.0} as i32,
                if position.is_some() {position.unwrap().y}else {60.0} as i32,
                (size.x * dpi_factor) as u32,
                (size.y * dpi_factor) as u32,
                0,
                (*visual_info).depth,
                xlib::InputOutput as u32,
                (*visual_info).visual,
                xlib::CWBorderPixel | xlib::CWColormap | xlib::CWEventMask, // | xlib::CWOverrideRedirect,
                &mut attributes,
            );
            
            // Tell the window manager that we want to be notified when the window is closed
            let mut wm_delete_message = (xlib.XInternAtom)(
                display,
                CString::new("WM_DELETE_WINDOW").unwrap().as_ptr(),
                xlib::False,
            );
            (xlib.XSetWMProtocols)(display, window, &mut wm_delete_message, 1);
            
            let hints_prop = (xlib.XInternAtom)(display, CString::new("_MOTIF_WM_HINTS").unwrap().as_ptr(), 0);
            let hints = MwmHints {
                flags: MWM_HINTS_DECORATIONS,
                functions: 0,
                decorations: 0,
                input_mode: 0,
                status: 0,
            };
            (xlib.XChangeProperty)(display, window, hints_prop, hints_prop, 32, xlib::PropModeReplace, &hints as *const _ as *const u8, 5);
            // Map the window to the screen
            (xlib.XMapWindow)(display, window);
            (xlib.XFlush)(display);
            
            // Create a window
            (*self.xlib_app).window_map.insert(window, self);
            
            self.attributes = Some(attributes);
            self.visual_info = Some(*visual_info);
            self.window = Some(window);
            self.last_window_geom = self.get_window_geom();
            
            (*self.xlib_app).event_recur_block = false;
            let new_geom = self.get_window_geom();
            self.do_callback(&mut vec![
                Event::WindowGeomChange(WindowGeomChangeEvent {
                    window_id: self.window_id,
                    old_geom: new_geom.clone(),
                    new_geom: new_geom
                })
            ]);
            (*self.xlib_app).event_recur_block = true;
        }
    }
    
    pub fn hide_child_windows(&mut self) {
        unsafe {
            let display = (*self.xlib_app).display;
            let xlib = &(*self.xlib_app).xlib;
            for child in &mut self.child_windows {
                if child.visible {
                    (xlib.XUnmapWindow)(display, child.window);
                    child.visible = false
                }
            }
        }
    }
    
    pub fn alloc_child_window(&mut self, x: i32, y: i32, w: u32, h: u32) -> Option<c_ulong> {
        unsafe {
            let display = (*self.xlib_app).display;
            let xlib = &(*self.xlib_app).xlib;
            
            // ok lets find a childwindow that matches x/y/w/h and show it if need be
            for child in &mut self.child_windows {
                if child.x == x && child.y == y && child.w == w && child.h == h {
                    if!child.visible {
                        (xlib.XMapWindow)(display, child.window);
                        child.visible = true;
                    }
                    (xlib.XRaiseWindow)(display, child.window);
                    return Some(child.window);
                }
            }
            
            for child in &mut self.child_windows {
                if !child.visible {
                    child.x = x;
                    child.y = y;
                    child.w = w;
                    child.h = h;
                    (xlib.XMoveResizeWindow)(display, child.window, x, y, w, h);
                    (xlib.XMapWindow)(display, child.window);
                    (xlib.XRaiseWindow)(display, child.window);
                    child.visible = true;
                    return Some(child.window);
                }
            }
            
            let new_child = (xlib.XCreateWindow)(
                display,
                self.window.unwrap(),
                x,
                y,
                w,
                h,
                0,
                self.visual_info.unwrap().depth,
                xlib::InputOutput as u32,
                self.visual_info.unwrap().visual,
                xlib::CWBorderPixel | xlib::CWColormap | xlib::CWEventMask | xlib::CWOverrideRedirect,
                self.attributes.as_mut().unwrap(),
            );
            
            // Map the window to the screen
            //(xlib.XMapWindow)(display, window_dirty);
            (*self.xlib_app).window_map.insert(new_child, self);
            (xlib.XMapWindow)(display, new_child);
            (xlib.XFlush)(display);
            
            self.child_windows.push(XlibChildWindow {
                window: new_child,
                x: x,
                y: y,
                w: w,
                h: h,
                visible: true
            });
            
            return Some(new_child)
            
        }
    }
    
    pub fn get_key_modifiers() -> KeyModifiers {
        //unsafe {
        KeyModifiers {
            control: false,
            shift: false,
            alt: false,
            logo: false
        }
        //}
    }
    
    pub fn update_ptrs(&mut self) {
        unsafe {
            (*self.xlib_app).window_map.insert(self.window.unwrap(), self);
            for i in 0..self.child_windows.len() {
                (*self.xlib_app).window_map.insert(self.child_windows[i].window, self);
            }
        }
    }
    
    pub fn on_mouse_move(&self) {
    }
    
    
    pub fn set_mouse_cursor(&mut self, _cursor: MouseCursor) {
    }
    
    pub fn restore(&self) {
    }
    
    pub fn maximize(&self) {
    }
    
    pub fn close_window(&self) {
    }
    
    pub fn minimize(&self) {
    }
    
    pub fn set_topmost(&self, _topmost: bool) {
    }
    
    pub fn get_is_topmost(&self) -> bool {
        false
    }
    
    pub fn get_window_geom(&self) -> WindowGeom {
        WindowGeom {
            is_topmost: self.get_is_topmost(),
            is_fullscreen: self.get_is_maximized(),
            inner_size: self.get_inner_size(),
            outer_size: self.get_outer_size(),
            dpi_factor: self.get_dpi_factor(),
            position: self.get_position()
        }
    }
    
    pub fn get_is_maximized(&self) -> bool {
        false
    }
    
    pub fn time_now(&self) -> f64 {
        let time_now = precise_time_ns();
        (time_now - self.time_start) as f64 / 1_000_000_000.0
    }
    
    pub fn set_ime_spot(&mut self, spot: Vec2) {
        self.ime_spot = spot;
    }
    
    pub fn get_position(&self) -> Vec2 {
        unsafe {
            let mut xwa = mem::uninitialized();
            let xlib = &(*self.xlib_app).xlib;
            let display = (*self.xlib_app).display;
            (xlib.XGetWindowAttributes)(display, self.window.unwrap(), &mut xwa);
            return Vec2 {x: xwa.x as f32, y: xwa.y as f32}
            /*
            let mut child = mem::uninitialized();
            let default_screen = (xlib.XDefaultScreen)(display);
            let root_window = (xlib.XRootWindow)(display, default_screen);
            let mut x:c_int = 0;
            let mut y:c_int = 0;
            (xlib.XTranslateCoordinates)(display, self.window.unwrap(), root_window, 0, 0, &mut x, &mut y, &mut child );
            */
        }
    }
    
    pub fn get_inner_size(&self) -> Vec2 {
        let dpi_factor = self.get_dpi_factor();
        unsafe {
            let mut xwa = mem::uninitialized();
            let xlib = &(*self.xlib_app).xlib;
            let display = (*self.xlib_app).display;
            (xlib.XGetWindowAttributes)(display, self.window.unwrap(), &mut xwa);
            return Vec2 {x: xwa.width as f32 / dpi_factor, y: xwa.height as f32 / dpi_factor}
        }
    }
    
    pub fn get_outer_size(&self) -> Vec2 {
        unsafe {
            let mut xwa = mem::uninitialized();
            let xlib = &(*self.xlib_app).xlib;
            let display = (*self.xlib_app).display;
            (xlib.XGetWindowAttributes)(display, self.window.unwrap(), &mut xwa);
            return Vec2 {x: xwa.width as f32, y: xwa.height as f32}
        }
    }
    
    pub fn set_position(&mut self, _pos: Vec2) {
    }
    
    pub fn set_outer_size(&self, _size: Vec2) {
    }
    
    pub fn set_inner_size(&self, _size: Vec2) {
    }
    
    pub fn get_dpi_factor(&self) -> f32 {
        unsafe {
            //return 2.0;
            let xlib = &(*self.xlib_app).xlib;
            let display = (*self.xlib_app).display;
            let resource_string = (xlib.XResourceManagerString)(display);
            let db = (xlib.XrmGetStringDatabase)(resource_string);
            let mut ty = mem::uninitialized();
            let mut value = mem::uninitialized();
            (xlib.XrmGetResource)(
                db,
                CString::new("Xft.dpi").unwrap().as_ptr(),
                CString::new("String").unwrap().as_ptr(),
                &mut ty,
                &mut value
            );
            if value.addr == std::ptr::null_mut() {
                return 2.0; // TODO find some other way to figure it out
            }
            else {
                let dpi: f32 = CStr::from_ptr(value.addr).to_str().unwrap().parse().unwrap();
                return dpi / 96.0;
            }
        }
    }
    
    pub fn do_callback(&mut self, events: &mut Vec<Event>) {
        unsafe {
            (*self.xlib_app).do_callback(events);
        }
    }
    
    pub fn send_change_event(&mut self) {
        
        let new_geom = self.get_window_geom();
        let old_geom = self.last_window_geom.clone();
        self.last_window_geom = new_geom.clone();
        
        self.do_callback(&mut vec![
            Event::WindowGeomChange(WindowGeomChangeEvent {
                window_id: self.window_id,
                old_geom: old_geom,
                new_geom: new_geom
            }),
            Event::Paint
        ]);
    }
    
    pub fn send_focus_event(&mut self) {
        self.do_callback(&mut vec![Event::AppFocus]);
    }
    
    pub fn send_focus_lost_event(&mut self) {
        self.do_callback(&mut vec![Event::AppFocusLost]);
    }
    
    pub fn send_finger_down(&mut self, digit: usize, modifiers: KeyModifiers) {
        let mut down_count = 0;
        for is_down in &self.fingers_down {
            if *is_down {
                down_count += 1;
            }
        }
        if down_count == 0 {
            //unsafe {winuser::SetCapture(self.hwnd.unwrap());}
        }
        self.fingers_down[digit] = true;
        self.do_callback(&mut vec![Event::FingerDown(FingerDownEvent {
            window_id: self.window_id,
            abs: self.last_mouse_pos,
            rel: self.last_mouse_pos,
            rect: Rect::zero(),
            digit: digit,
            handled: false,
            is_touch: false,
            modifiers: modifiers,
            tap_count: 0,
            time: self.time_now()
        })]);
    }
    
    pub fn send_finger_up(&mut self, digit: usize, modifiers: KeyModifiers) {
        self.fingers_down[digit] = false;
        let mut down_count = 0;
        for is_down in &self.fingers_down {
            if *is_down {
                down_count += 1;
            }
        }
        if down_count == 0 {
            // unsafe {winuser::ReleaseCapture();}
        }
        self.do_callback(&mut vec![Event::FingerUp(FingerUpEvent {
            window_id: self.window_id,
            abs: self.last_mouse_pos,
            rel: self.last_mouse_pos,
            rect: Rect::zero(),
            abs_start: Vec2::zero(),
            rel_start: Vec2::zero(),
            digit: digit,
            is_over: false,
            is_touch: false,
            modifiers: modifiers,
            time: self.time_now()
        })]);
    }
    
    pub fn send_finger_hover_and_move(&mut self, pos: Vec2, modifiers: KeyModifiers) {
        self.last_mouse_pos = pos;
        let mut events = Vec::new();
        for (digit, down) in self.fingers_down.iter().enumerate() {
            if *down {
                events.push(Event::FingerMove(FingerMoveEvent {
                    window_id: self.window_id,
                    abs: pos,
                    rel: pos,
                    rect: Rect::zero(),
                    digit: digit,
                    abs_start: Vec2::zero(),
                    rel_start: Vec2::zero(),
                    is_over: false,
                    is_touch: false,
                    modifiers: modifiers.clone(),
                    time: self.time_now()
                }));
            }
        };
        events.push(Event::FingerHover(FingerHoverEvent {
            window_id: self.window_id,
            abs: pos,
            rel: pos,
            any_down: false,
            rect: Rect::zero(),
            handled: false,
            hover_state: HoverState::Over,
            modifiers: modifiers,
            time: self.time_now()
        }));
        self.do_callback(&mut events);
    }
    
    pub fn send_close_requested_event(&mut self) -> bool {
        let mut events = vec![Event::WindowCloseRequested(WindowCloseRequestedEvent {window_id: self.window_id, accept_close: true})];
        self.do_callback(&mut events);
        if let Event::WindowCloseRequested(cre) = &events[0] {
            return cre.accept_close
        }
        true
    }
    
    pub fn send_text_input(&mut self, input: String, replace_last: bool) {
        self.do_callback(&mut vec![Event::TextInput(TextInputEvent {
            input: input,
            was_paste: false,
            replace_last: replace_last
        })])
    }
    
}

#[derive(Debug, Clone, Copy, PartialEq)]
#[repr(C)]
struct MwmHints {
    pub flags: c_ulong,
    pub functions: c_ulong,
    pub decorations: c_ulong,
    pub input_mode: c_long,
    pub status: c_ulong,
}

const MWM_HINTS_FUNCTIONS: c_ulong = (1 << 0);
const MWM_HINTS_DECORATIONS: c_ulong = (1 << 1);

const MWM_FUNC_ALL: c_ulong = (1 << 0);
const MWM_FUNC_RESIZE: c_ulong = (1 << 1);
const MWM_FUNC_MOVE: c_ulong = (1 << 2);
const MWM_FUNC_MINIMIZE: c_ulong = (1 << 3);
const MWM_FUNC_MAXIMIZE: c_ulong = (1 << 4);
const MWM_FUNC_CLOSE: c_ulong = (1 << 5);
const _NET_WM_MOVERESIZE_SIZE_TOPLEFT: c_long = 0;
const _NET_WM_MOVERESIZE_SIZE_TOP: c_long = 1;
const _NET_WM_MOVERESIZE_SIZE_TOPRIGHT: c_long = 2;
const _NET_WM_MOVERESIZE_SIZE_RIGHT: c_long = 3;
const _NET_WM_MOVERESIZE_SIZE_BOTTOMRIGHT: c_long = 4;
const _NET_WM_MOVERESIZE_SIZE_BOTTOM: c_long = 5;
const _NET_WM_MOVERESIZE_SIZE_BOTTOMLEFT: c_long = 6;
const _NET_WM_MOVERESIZE_SIZE_LEFT: c_long = 7;
const _NET_WM_MOVERESIZE_MOVE: c_long = 8;/* movement only */
const _NET_WM_MOVERESIZE_SIZE_KEYBOARD: c_long = 9;/* size via keyboard */
const _NET_WM_MOVERESIZE_MOVE_KEYBOARD: c_long = 10;
/* move via keyboard */