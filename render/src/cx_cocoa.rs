// Life is too short for leaky abstractions.
// Gleaned/learned/templated from https://github.com/tomaka/winit/blob/master/src/platform/macos/

use std::collections::HashMap;

use cocoa::base::{id, nil};
use cocoa::{appkit, foundation};
use cocoa::appkit::{NSStringPboardType, NSApplication, NSEventModifierFlags, NSImage, NSEvent, NSWindow, NSView,NSEventMask,NSRunningApplication};
use cocoa::foundation::{NSArray, NSPoint, NSDictionary, NSRect, NSSize, NSUInteger,NSInteger};
use cocoa::foundation::{NSAutoreleasePool, NSString};
use objc::runtime::{Class, Object, Protocol, Sel, BOOL, YES, NO};
use objc::declare::ClassDecl;
use std::os::raw::c_void;
use objc::*;
use core_graphics::display::CGDisplay;
use time::precise_time_ns;

use crate::cx::*;

#[derive(Clone, Default)]
pub struct CocoaWindow{
    pub window_delegate:Option<id>,
    pub view:Option<id>,
    pub window:Option<id>,
    pub attributes_for_marked_text:Option<id>,
    pub empty_string:Option<id>,
    //pub input_context:Option<id>,
    pub pasteboard:Option<id>,
    pub init_resize:bool,
    pub last_size:Vec2,
    pub ime_spot:Vec2,
    pub last_dpi_factor:f32,
    pub time_start:u64,
    pub last_key_mod:KeyModifiers,
    pub fingers_down:Vec<bool>,
    pub cursors:HashMap<MouseCursor, id>,
    pub current_cursor:MouseCursor,
    pub last_mouse_pos:Vec2,
    pub event_callback:Option<*mut FnMut(&mut Vec<Event>)>
}

impl CocoaWindow{
    pub fn cocoa_app_init(){
        unsafe{
            let ns_app = appkit::NSApp();
            if ns_app == nil {
                panic!("App is nil");
            } 
            ns_app.setActivationPolicy_( appkit::NSApplicationActivationPolicy::NSApplicationActivationPolicyRegular);
            ns_app.finishLaunching();
            let current_app = appkit::NSRunningApplication::currentApplication(nil);
            current_app.activateWithOptions_(appkit::NSApplicationActivateIgnoringOtherApps);
        }
    }

    pub fn set_mouse_cursor(&mut self, cursor:MouseCursor){
        if self.current_cursor != cursor{
            self.current_cursor = cursor;
            unsafe{
                let _: () = msg_send![
                    self.window.unwrap(),
                    invalidateCursorRectsForView:self.view.unwrap()
                ];
            }
        }
    }

    pub fn init(&mut self, title:&str){
        unsafe{

            let objects = vec![
                NSString::alloc(nil).init_str("NSMarkedClauseSegment"),
                NSString::alloc(nil).init_str("NSGlyphInfo")
            ];
            self.attributes_for_marked_text = Some(NSArray::arrayWithObjects(nil, &objects));
            self.empty_string = Some(NSString::alloc(nil).init_str(""));

            for _i in 0..10{
                self.fingers_down.push(false);
            }
            let autoreleasepool = NSAutoreleasePool::new(nil);
            // construct a window with our class
            let window_class = define_cocoa_window_class();
            let window:id = msg_send![window_class, alloc];
            //let window_id:id = msg_send![window_class, alloc];
            
            let window_frame = NSRect::new(NSPoint::new(0., 0.), NSSize::new(800., 600.));
            let window_masks = appkit::NSWindowStyleMask::NSClosableWindowMask |
                appkit::NSWindowStyleMask::NSMiniaturizableWindowMask |
                appkit::NSWindowStyleMask::NSResizableWindowMask |
                appkit::NSWindowStyleMask::NSTitledWindowMask;

            window.initWithContentRect_styleMask_backing_defer_(
                window_frame,
                window_masks,
                appkit::NSBackingStoreBuffered,
                NO
            );
            
            let window_delegate_class = define_cocoa_window_delegate();
            let window_delegate:id = msg_send![window_delegate_class, new];

            (*window_delegate).set_ivar("cocoa_window_ptr", self as *mut _ as *mut c_void);
    
            msg_send![window, setDelegate:window_delegate];            

            let title = NSString::alloc(nil).init_str(title);
            window.setReleasedWhenClosed_(NO);
            window.setTitle_(title);
            window.setAcceptsMouseMovedEvents_(YES);

            let view_class = define_cocoa_view_class();
            let view:id = msg_send![view_class, alloc];
            msg_send![view, initWithPtr:self as *mut _ as *mut c_void];
                                   
            window.setContentView_(view);
            window.makeFirstResponder_(view);
            window.makeKeyAndOrderFront_(nil);

            //let input_context: id = msg_send![class!(NSTextInputContext), alloc];
            //msg_send![input_context, initWithClient:view];
            window.center();

            self.window_delegate = Some(window_delegate);
            self.window = Some(window);
            self.view = Some(view);
            //self.input_context = Some(input_context);
            self.last_size = self.get_inner_size();
            self.last_dpi_factor = self.get_dpi_factor();
            self.time_start = precise_time_ns();

            let _: () = msg_send![autoreleasepool, drain];

            let input_context:id = msg_send![view, inputContext];
            msg_send![input_context, invalidateCharacterCoordinates];

            // grab the pasteboard
            let pasteboard:id = msg_send![class!(NSPasteboard), generalPasteboard];
            self.pasteboard = Some(pasteboard);
            //msg_send![input_context, activate];
        }
    }

    pub fn time_now(&self)->f64{
        let time_now = precise_time_ns();
        (time_now - self.time_start) as f64 / 1_000_000_000.0
    }

    pub fn set_position(&mut self, pos:Vec2){
        let ns_point =  NSPoint::new(pos.x as f64,CGDisplay::main().pixels_high() as f64 - pos.y as f64);
        unsafe {
            NSWindow::setFrameTopLeftPoint_(self.window.unwrap(), ns_point);
        }        
    }

    pub fn get_position(&mut self)->Vec2{
        //let view_frame = unsafe { NSView::frame(self.view.unwrap()) };
        let rect = NSRect::new(
            NSPoint::new(0.0, 0.0),//view_frame.size.height),
            NSSize::new(0.0, 0.0),
        );
        let out = unsafe{NSWindow::convertRectToScreen_(self.window.unwrap(), rect)};
        Vec2{x:out.origin.x as f32, y:out.origin.y as f32}
    }

    pub fn get_inner_size(&self)->Vec2{
        if self.view.is_none(){
            return Vec2::zero();
        }
        let view_frame = unsafe { NSView::frame(self.view.unwrap()) };
        Vec2{x:view_frame.size.width as f32, y:view_frame.size.height as f32}
    }

    pub fn get_dpi_factor(&self)->f32{
        if self.window.is_none(){
            return 1.0;
        }
        unsafe{NSWindow::backingScaleFactor(self.window.unwrap()) as f32}
    }

    unsafe fn process_ns_event(&mut self, ns_event: cocoa::base::id){
        if ns_event == cocoa::base::nil {
            return;
        }

        if ns_event.eventType() as u64 == 21 { // some missing event from cocoa-rs crate
            return;
        }

        appkit::NSApp().sendEvent_(ns_event);

        match ns_event.eventType(){
            appkit::NSKeyUp => {
                if let Some(key_code) = get_event_keycode(ns_event){
                    let modifiers = get_event_key_modifier(ns_event);
                    let key_char = get_event_char(ns_event);
                    let is_repeat:bool = msg_send![ns_event, isARepeat];
                    self.do_callback(&mut vec![
                        Event::KeyUp(KeyEvent{
                            key_code:key_code,
                            key_char:key_char,
                            is_repeat:is_repeat,
                            modifiers:modifiers,
                            time:self.time_now()
                        })
                    ]);
                }
            },
            appkit::NSKeyDown => {
                if let Some(key_code) = get_event_keycode(ns_event){
                    let modifiers = get_event_key_modifier(ns_event);
                    let key_char = get_event_char(ns_event);
                    let is_repeat:bool = msg_send![ns_event, isARepeat];
                    let is_return = if let KeyCode::Return = key_code{true} else{false};

                    // see if its is paste, ifso TextInput was_paste the text
                    let paste_text = if let KeyCode::KeyV = key_code{
                        if modifiers.logo || modifiers.control{
                            // was a paste
                            let nsstring:id = msg_send![self.pasteboard.unwrap(), stringForType:NSStringPboardType];
                            let string = nsstring_to_string(nsstring);
                            
                            Some(string)
                        }
                        else{None}
                    }
                    else{None};

                    match key_code{
                        KeyCode::KeyX | KeyCode::KeyC=>if modifiers.logo || modifiers.control{
                            // cut or copy.
                            let mut events = vec![
                                Event::TextCopy(TextCopyEvent{
                                    response:None
                                })
                            ];
                            self.do_callback(&mut events);
                            match &events[0]{
                                Event::TextCopy(req)=>if let Some(response) = &req.response{
                                    // plug it into the apple clipboard
                                    let nsstring:id = NSString::alloc(nil).init_str(&response);
                                    let array: id = msg_send![class!(NSArray), arrayWithObject:NSStringPboardType];
                                    msg_send![self.pasteboard.unwrap(), declareTypes:array owner:nil];
                                    msg_send![self.pasteboard.unwrap(), setString:nsstring forType:NSStringPboardType];
                                },
                                _=>()
                            };
                        },
                        _=>{}
                    }

                    self.do_callback(&mut vec![
                        Event::KeyDown(KeyEvent{
                            key_code:key_code,
                            key_char:key_char,
                            is_repeat:is_repeat,
                            modifiers:modifiers,
                            time:self.time_now()
                        })
                    ]);
                    if is_return{
                        self.do_callback(&mut vec![
                            Event::TextInput(TextInputEvent{
                                input:"\n".to_string(),
                                was_paste:false,
                                replace_last:false
                            })
                        ]);
                    }
                    if let Some(paste_text) = paste_text{
                        self.do_callback(&mut vec![
                            Event::TextInput(TextInputEvent{
                                input:paste_text,
                                was_paste:true,
                                replace_last:false
                            })
                        ]);
                    }
                    

                }
            },
            appkit::NSFlagsChanged => {
                let modifiers = get_event_key_modifier(ns_event);
                let last_key_mod = self.last_key_mod.clone();
                self.last_key_mod = modifiers.clone();
                let mut events = Vec::new();
                fn add_event(time:f64, old:bool, new:bool, modifiers:KeyModifiers, events:&mut Vec<Event>, key_code:KeyCode){
                    if old != new{
                        let event = KeyEvent{
                            key_code:key_code,
                            key_char:'\0',
                            is_repeat:false,
                            modifiers:modifiers,
                            time:time
                        };
                        if new{
                            events.push(Event::KeyDown(event));
                        }
                        else{
                            events.push(Event::KeyUp(event));
                        }
                    }
                }
                let time = self.time_now();
                add_event(time, last_key_mod.shift, modifiers.shift, modifiers.clone(), &mut events, KeyCode::Shift);
                add_event(time, last_key_mod.alt, modifiers.alt, modifiers.clone(), &mut events, KeyCode::Alt);
                add_event(time, last_key_mod.logo, modifiers.logo, modifiers.clone(), &mut events, KeyCode::Logo);
                add_event(time, last_key_mod.control, modifiers.control, modifiers.clone(), &mut events, KeyCode::Control);
                if events.len() > 0{
                    self.do_callback(&mut events);
                }
            },
            appkit::NSMouseEntered => {},
            appkit::NSMouseExited => {},
            appkit::NSMouseMoved |
            appkit::NSLeftMouseDragged |
            appkit::NSOtherMouseDragged |
            appkit::NSRightMouseDragged => {},
            appkit::NSScrollWheel => {
                return if ns_event.hasPreciseScrollingDeltas() == cocoa::base::YES {
                    self.do_callback(&mut vec![
                        Event::FingerScroll(FingerScrollEvent{
                            scroll:Vec2{
                                x:-ns_event.scrollingDeltaX() as f32,
                                y:-ns_event.scrollingDeltaY() as f32
                            },
                            abs:self.last_mouse_pos,
                            rel:self.last_mouse_pos,
                            rect:Rect::zero(),
                            is_wheel:false,
                            modifiers:get_event_key_modifier(ns_event),
                            handled:false,
                            time:self.time_now()
                        })
                    ]);
                } else {
                    self.do_callback(&mut vec![
                        Event::FingerScroll(FingerScrollEvent{
                            scroll:Vec2{
                                x:-ns_event.scrollingDeltaX() as f32 * 32.,
                                y:-ns_event.scrollingDeltaY() as f32 * 32.
                            },
                            abs:self.last_mouse_pos,
                            rel:self.last_mouse_pos,
                            rect:Rect::zero(),
                            is_wheel:true,
                            modifiers:get_event_key_modifier(ns_event),
                            handled:false,
                            time:self.time_now()
                        })
                    ]);
                }
            },
            appkit::NSEventTypePressure => {},
            appkit::NSApplicationDefined => match ns_event.subtype() {
                appkit::NSEventSubtype::NSApplicationActivatedEventType => {
                },
                _=>(),
            },
            _=>(),
        }
    }

    pub fn poll_events<F>(&mut self, first_block:bool, mut event_handler:F)
    where F: FnMut(&mut Vec<Event>),
    {   
        let mut do_first_block = first_block;

        unsafe{
            self.event_callback = Some(&mut event_handler as *const FnMut(&mut Vec<Event>) as *mut FnMut(&mut Vec<Event>));

            if !self.init_resize{
                self.init_resize = true;
                self.send_resize_event();
            }

            loop{
                let pool = foundation::NSAutoreleasePool::new(cocoa::base::nil);

                let ns_event = appkit::NSApp().nextEventMatchingMask_untilDate_inMode_dequeue_(
                    NSEventMask::NSAnyEventMask.bits() | NSEventMask::NSEventMaskPressure.bits(),
                    if do_first_block{
                        do_first_block = false;
                        foundation::NSDate::distantFuture(cocoa::base::nil)
                    }
                    else{
                        foundation::NSDate::distantPast(cocoa::base::nil)
                    },
                    foundation::NSDefaultRunLoopMode,
                    cocoa::base::YES);
                
                if ns_event == nil{
                    break;
                }
               // msg_send![self.input_context.unwrap(), handleEvent:ns_event];
                self.process_ns_event(ns_event);

                let _: () = msg_send![pool, release];

                //if !event.is_none(){
                //    event_handler(&mut vec![event.unwrap()])
                // }
            }
            self.event_callback = None;
        }
    }

    pub fn do_callback(&mut self, events:&mut Vec<Event>){
        unsafe{
            if self.event_callback.is_none(){
                return
            };
            let callback = self.event_callback.unwrap();
            (*callback)(events);
        }
    }

    pub fn send_resize_event(&mut self){
        let new_dpi_factor = self.get_dpi_factor();
        let new_size = self.get_inner_size(); 
        let old_dpi_factor = self.last_dpi_factor;
        let old_size = self.last_size;
        self.last_dpi_factor = new_dpi_factor;
        self.last_size = new_size;
        self.do_callback(&mut vec![Event::Resized(ResizedEvent{
            old_size:old_size,
            old_dpi_factor:old_dpi_factor,
            new_size:new_size,
            new_dpi_factor:new_dpi_factor
        })]);
    }

    pub fn send_focus_event(&mut self, focus:bool){
        self.do_callback(&mut vec![Event::AppFocus(focus)]);
    }

    pub fn send_finger_down(&mut self, digit:usize, modifiers:KeyModifiers){
        self.fingers_down[digit] = true;
        self.do_callback(&mut vec![Event::FingerDown(FingerDownEvent{
            abs:self.last_mouse_pos,
            rel:self.last_mouse_pos,
            rect:Rect::zero(),
            digit:digit,
            handled:false,
            is_touch:false,
            modifiers:modifiers,
            tap_count:0,
            time:self.time_now()
        })]);
    }

    pub fn send_finger_up(&mut self, digit:usize, modifiers:KeyModifiers){
        self.fingers_down[digit] = false;
        self.do_callback(&mut vec![Event::FingerUp(FingerUpEvent{
            abs:self.last_mouse_pos,
            rel:self.last_mouse_pos,
            rect:Rect::zero(),
            abs_start:Vec2::zero(),
            rel_start:Vec2::zero(),
            digit:digit,
            is_over:false,
            is_touch:false,
            modifiers:modifiers,
            time:self.time_now()
        })]);
    }

    pub fn send_finger_hover_and_move(&mut self, pos:Vec2, modifiers:KeyModifiers){
        self.last_mouse_pos = pos;
        let mut events = Vec::new();
        for (digit, down) in self.fingers_down.iter().enumerate(){
            if *down{
                events.push(Event::FingerMove(FingerMoveEvent{
                    abs:pos,
                    rel:pos,
                    rect:Rect::zero(),
                    digit:digit,
                    abs_start:Vec2::zero(),
                    rel_start:Vec2::zero(),
                    is_over:false,
                    is_touch:false,
                    modifiers:modifiers.clone(),
                    time:self.time_now()
                }));
            }
        };
        events.push(Event::FingerHover(FingerHoverEvent{
            abs:pos,
            rel:pos,
            rect:Rect::zero(),
            handled:false,
            hover_state:HoverState::Over,
            modifiers:modifiers,
            time:self.time_now()
        }));
        self.do_callback(&mut events);
    }

    pub fn send_close_requested_event(&mut self){
        self.do_callback(&mut vec![Event::CloseRequested])
    }

    pub fn send_text_input(&mut self, input:String, replace_last:bool){
        self.do_callback(&mut vec![Event::TextInput(TextInputEvent{
            input:input,
            was_paste:false,
            replace_last:replace_last
        })])
    }
}

fn get_event_char(event: id) -> char {
    unsafe {
        let characters: id =  msg_send![event, characters];
        if characters == nil{
            return '\0'
        }
        if characters.len() == 0{
            return '\0'
        }
        let slice = std::slice::from_raw_parts(
            characters.UTF8String() as *const std::os::raw::c_uchar,
            characters.len(),
        );
        let string = std::str::from_utf8_unchecked(slice);
        string.chars().next().unwrap()
    }
}

fn get_event_key_modifier(event: id) -> KeyModifiers {
    let flags = unsafe {
        NSEvent::modifierFlags(event)
    };
    KeyModifiers {
        shift: flags.contains(NSEventModifierFlags::NSShiftKeyMask),
        control: flags.contains(NSEventModifierFlags::NSControlKeyMask),
        alt: flags.contains(NSEventModifierFlags::NSAlternateKeyMask),
        logo: flags.contains(NSEventModifierFlags::NSCommandKeyMask),
    }
}

fn get_event_keycode(event: id) -> Option<KeyCode> {
    let scan_code: std::os::raw::c_ushort = unsafe {
        msg_send![event, keyCode]
    };
    //println!("SCAN CODE {} ", scan_code);
    Some(match scan_code {
        0x00 => KeyCode::KeyA,
        0x01 => KeyCode::KeyS,
        0x02 => KeyCode::KeyD,
        0x03 => KeyCode::KeyF,
        0x04 => KeyCode::KeyH,
        0x05 => KeyCode::KeyG,
        0x06 => KeyCode::KeyZ,
        0x07 => KeyCode::KeyX,
        0x08 => KeyCode::KeyC,
        0x09 => KeyCode::KeyV,
        //0x0a => World 1,
        0x0b => KeyCode::KeyB,
        0x0c => KeyCode::KeyQ,
        0x0d => KeyCode::KeyW,
        0x0e => KeyCode::KeyE,
        0x0f => KeyCode::KeyR,
        0x10 => KeyCode::KeyY,
        0x11 => KeyCode::KeyT,
        0x12 => KeyCode::Key1,
        0x13 => KeyCode::Key2,
        0x14 => KeyCode::Key3,
        0x15 => KeyCode::Key4,
        0x16 => KeyCode::Key6,
        0x17 => KeyCode::Key5,
        0x18 => KeyCode::Equals,
        0x19 => KeyCode::Key9,
        0x1a => KeyCode::Key7,
        0x1b => KeyCode::Minus,
        0x1c => KeyCode::Key8,
        0x1d => KeyCode::Key0,
        0x1e => KeyCode::RBracket,
        0x1f => KeyCode::KeyO,
        0x20 => KeyCode::KeyU,
        0x21 => KeyCode::LBracket,
        0x22 => KeyCode::KeyI,
        0x23 => KeyCode::KeyP,
        0x24 => KeyCode::Return,
        0x25 => KeyCode::KeyL,
        0x26 => KeyCode::KeyJ,
        0x27 => KeyCode::Backtick,
        0x28 => KeyCode::KeyK,
        0x29 => KeyCode::Semicolon,
        0x2a => KeyCode::Backslash,
        0x2b => KeyCode::Comma,
        0x2c => KeyCode::Slash,
        0x2d => KeyCode::KeyN,
        0x2e => KeyCode::KeyM,
        0x2f => KeyCode::Period,
        0x30 => KeyCode::Tab,
        0x31 => KeyCode::Space,
        0x32 => KeyCode::Backtick,
        0x33 => KeyCode::Backspace,
        //0x34 => unkown,
        0x35 => KeyCode::Escape,
        //0x36 => KeyCode::RLogo,
        //0x37 => KeyCode::LLogo,
        //0x38 => KeyCode::LShift,
        0x39 => KeyCode::Capslock,
        //0x3a => KeyCode::LAlt,
        //0x3b => KeyCode::LControl,
        //0x3c => KeyCode::RShift,
        //0x3d => KeyCode::RAlt,
        //0x3e => KeyCode::RControl,
        //0x3f => Fn key,
        //0x40 => KeyCode::F17,
        0x41 => KeyCode::NumpadDecimal,
        //0x42 -> unkown,
        0x43 => KeyCode::NumpadMultiply,
        //0x44 => unkown,
        0x45 => KeyCode::NumpadAdd,
        //0x46 => unkown,
        0x47 => KeyCode::Numlock,
        //0x48 => KeypadClear,
        //0x49 => KeyCode::VolumeUp,
        //0x4a => KeyCode::VolumeDown,
        0x4b => KeyCode::NumpadDivide,
        0x4c => KeyCode::NumpadEnter,
        0x4e => KeyCode::NumpadSubtract,
        //0x4d => unkown,
        //0x4e => KeyCode::Subtract,
        //0x4f => KeyCode::F18,
        //0x50 => KeyCode::F19,
        0x51 => KeyCode::NumpadEquals,
        0x52 => KeyCode::Numpad0,
        0x53 => KeyCode::Numpad1,
        0x54 => KeyCode::Numpad2,
        0x55 => KeyCode::Numpad3,
        0x56 => KeyCode::Numpad4,
        0x57 => KeyCode::Numpad5,
        0x58 => KeyCode::Numpad6,
        0x59 => KeyCode::Numpad7,
        //0x5a => KeyCode::F20,
        0x5b => KeyCode::Numpad8,
        0x5c => KeyCode::Numpad9,
        //0x5d => KeyCode::Yen,
        //0x5e => JIS Ro,
        //0x5f => unkown,
        0x60 => KeyCode::F5,
        0x61 => KeyCode::F6,
        0x62 => KeyCode::F7,
        0x63 => KeyCode::F3,
        0x64 => KeyCode::F8,
        0x65 => KeyCode::F9,
        //0x66 => JIS Eisuu (macOS),
        0x67 => KeyCode::F11,
        //0x68 => JIS Kana (macOS),
        //0x69 => KeyCode::F13,
        //0x6a => KeyCode::F16,
        //0x6b => KeyCode::F14,
        //0x6c => unkown,
        0x6d => KeyCode::F10,
        //0x6e => unkown,
        0x6f => KeyCode::F12,
        //0x70 => unkown,
        //0x71 => KeyCode::F15,
        0x72 => KeyCode::Insert,
        0x73 => KeyCode::Home,
        0x74 => KeyCode::PageUp,
        0x75 => KeyCode::Delete,
        0x76 => KeyCode::F4,
        0x77 => KeyCode::End,
        0x78 => KeyCode::F2,
        0x79 => KeyCode::PageDown,
        0x7a => KeyCode::F1,
        0x7b => KeyCode::ArrowLeft,
        0x7c => KeyCode::ArrowRight,
        0x7d => KeyCode::ArrowDown,
        0x7e => KeyCode::ArrowUp,
        //0x7f =>  unkown,
        //0xa => KeyCode::Caret,
        _ => return None,
    })
}

fn get_cocoa_window(this:&Object)->&mut CocoaWindow{
    unsafe{
        let ptr: *mut c_void = *this.get_ivar("cocoa_window_ptr");
        &mut *(ptr as *mut CocoaWindow)
    }
}

pub fn define_cocoa_window_delegate()->*const Class{
    use std::os::raw::c_void;
    
    extern fn window_should_close(this: &Object, _: Sel, _: id) -> BOOL {
        let cw = get_cocoa_window(this);
        cw.send_close_requested_event();
        NO
    }

    extern fn window_will_close(this: &Object, _: Sel, _: id) {
        let _cw = get_cocoa_window(this);
    }

    extern fn window_did_resize(this: &Object, _: Sel, _: id) {
        let cw = get_cocoa_window(this);
        cw.send_resize_event();
    }

    // This won't be triggered if the move was part of a resize.
    extern fn window_did_move(this: &Object, _: Sel, _: id) {
        let _cw = get_cocoa_window(this);
    }

    extern fn window_did_change_screen(this: &Object, _: Sel, _: id) {
        let cw = get_cocoa_window(this);
        cw.send_resize_event();
    }

    // This will always be called before `window_did_change_screen`.
    extern fn window_did_change_backing_properties(this: &Object, _:Sel, _:id) {
        let cw = get_cocoa_window(this);
        cw.send_resize_event();
    }

    extern fn window_did_become_key(this: &Object, _: Sel, _: id) {
        let cw = get_cocoa_window(this);
        cw.send_focus_event(true);
    }

    extern fn window_did_resign_key(this: &Object, _: Sel, _: id) {
        let cw = get_cocoa_window(this);
        cw.send_focus_event(false);
    }

    // Invoked when the dragged image enters destination bounds or frame
    extern fn dragging_entered(_this: &Object, _: Sel, _sender: id) -> BOOL {
        YES
    }

    // Invoked when the image is released
    extern fn prepare_for_drag_operation(_: &Object, _: Sel, _: id) -> BOOL {
        YES
    }

    // Invoked after the released image has been removed from the screen
    extern fn perform_drag_operation(_this: &Object, _: Sel, _sender: id) -> BOOL {
        YES
    }

    // Invoked when the dragging operation is complete
    extern fn conclude_drag_operation(_: &Object, _: Sel, _: id) {}

    // Invoked when the dragging operation is cancelled
    extern fn dragging_exited(this: &Object, _: Sel, _: id) {
        let _cw = get_cocoa_window(this);
        //WindowDelegate::emit_event(state, WindowEvent::HoveredFileCancelled);
    }

    // Invoked when entered fullscreen
    extern fn window_did_enter_fullscreen(this: &Object, _: Sel, _: id){
        let cw = get_cocoa_window(this);
        cw.send_resize_event();
    }

    // Invoked when before enter fullscreen
    extern fn window_will_enter_fullscreen(this: &Object, _: Sel, _: id) {
        let _cw = get_cocoa_window(this);
    }

    // Invoked when exited fullscreen
    extern fn window_did_exit_fullscreen(this: &Object, _: Sel, _: id){
        let cw = get_cocoa_window(this);
        cw.send_resize_event();
    }

    extern fn window_did_fail_to_enter_fullscreen(_this: &Object, _: Sel, _: id) {
    }

    let superclass = class!(NSObject);
    let mut decl = ClassDecl::new("RenderWindowDelegate", superclass).unwrap();

    // Add callback methods
    unsafe{
        decl.add_method(sel!(windowShouldClose:), window_should_close as extern fn(&Object, Sel, id) -> BOOL);
        decl.add_method(sel!(windowWillClose:), window_will_close as extern fn(&Object, Sel, id));
        decl.add_method(sel!(windowDidResize:), window_did_resize as extern fn(&Object, Sel, id));
        decl.add_method(sel!(windowDidMove:), window_did_move as extern fn(&Object, Sel, id));
        decl.add_method(sel!(windowDidChangeScreen:), window_did_change_screen as extern fn(&Object, Sel, id));
        decl.add_method(sel!(windowDidChangeBackingProperties:), window_did_change_backing_properties as extern fn(&Object, Sel, id));
        decl.add_method(sel!(windowDidBecomeKey:), window_did_become_key as extern fn(&Object, Sel, id));
        decl.add_method(sel!(windowDidResignKey:), window_did_resign_key as extern fn(&Object, Sel, id));

        // callbacks for drag and drop events
        decl.add_method(sel!(draggingEntered:), dragging_entered as extern fn(&Object, Sel, id) -> BOOL);
        decl.add_method(sel!(prepareForDragOperation:), prepare_for_drag_operation as extern fn(&Object, Sel, id) -> BOOL);
        decl.add_method(sel!(performDragOperation:), perform_drag_operation as extern fn(&Object, Sel, id) -> BOOL);
        decl.add_method(sel!(concludeDragOperation:), conclude_drag_operation as extern fn(&Object, Sel, id));
        decl.add_method(sel!(draggingExited:), dragging_exited as extern fn(&Object, Sel, id));

        // callbacks for fullscreen events
        decl.add_method(sel!(windowDidEnterFullScreen:), window_did_enter_fullscreen as extern fn(&Object, Sel, id));
        decl.add_method(sel!(windowWillEnterFullScreen:), window_will_enter_fullscreen as extern fn(&Object, Sel, id));
        decl.add_method(sel!(windowDidExitFullScreen:), window_did_exit_fullscreen as extern fn(&Object, Sel, id));
        decl.add_method(sel!(windowDidFailToEnterFullScreen:), window_did_fail_to_enter_fullscreen as extern fn(&Object, Sel, id));
    }
    // Store internal state as user data
    decl.add_ivar::<*mut c_void>("cocoa_window_ptr");

    return decl.register();
}

pub fn define_cocoa_window_class()->*const Class{
    extern fn yes(_: &Object, _: Sel) -> BOOL {
        YES
    }
    
    let window_superclass = class!(NSWindow);
    let mut decl = ClassDecl::new("RenderWindow", window_superclass).unwrap();
    unsafe{
        decl.add_method(sel!(canBecomeMainWindow), yes as extern fn(&Object, Sel) -> BOOL);
        decl.add_method(sel!(canBecomeKeyWindow), yes as extern fn(&Object, Sel) -> BOOL);
    }
    return decl.register();
}

pub fn define_cocoa_view_class()->*const Class{

    extern fn dealloc(this: &Object, _sel: Sel) {
        unsafe {
            let marked_text: id = *this.get_ivar("markedText");
            let _: () = msg_send![marked_text, release];
        }
    }

    extern fn init_with_ptr(this: &Object, _sel: Sel, cx: *mut c_void) -> id {
        unsafe {
            let this: id = msg_send![this, init];
            if this != nil {
                (*this).set_ivar("cocoa_window_ptr", cx);
                let marked_text = <id as NSMutableAttributedString>::init(
                    NSMutableAttributedString::alloc(nil),
                );
                (*this).set_ivar("markedText", marked_text);
            }
            this
        }
    }

    extern fn mouse_down(this: &Object, _sel: Sel, event: id) {
        let cw = get_cocoa_window(this);
        let modifiers = get_event_key_modifier(event);
        cw.send_finger_down(0, modifiers);
    }

    extern fn mouse_up(this: &Object, _sel: Sel, event: id) {
        let cw = get_cocoa_window(this);
        let modifiers = get_event_key_modifier(event);
        cw.send_finger_up(0, modifiers);
    }

    extern fn right_mouse_down(this: &Object, _sel: Sel, event: id) {
        let cw = get_cocoa_window(this);
        let modifiers = get_event_key_modifier(event);
        cw.send_finger_down(1, modifiers);
    }

    extern fn right_mouse_up(this: &Object, _sel: Sel, event: id) {
        let cw = get_cocoa_window(this);
        let modifiers = get_event_key_modifier(event);
        cw.send_finger_up(1, modifiers);
    }

    extern fn other_mouse_down(this: &Object, _sel: Sel, event: id) {
        let cw = get_cocoa_window(this);
        let modifiers = get_event_key_modifier(event);
        cw.send_finger_down(2, modifiers);
    }

    extern fn other_mouse_up(this: &Object, _sel: Sel, event: id) {
        let cw = get_cocoa_window(this);
        let modifiers = get_event_key_modifier(event);
        cw.send_finger_up(2, modifiers);
    }

    fn mouse_pos_from_event(this: &Object, event: id)->Vec2{
        // We have to do this to have access to the `NSView` trait...
        unsafe{
            let view: id = this as *const _ as *mut _;
            let window_point = event.locationInWindow();
            let view_point = view.convertPoint_fromView_(window_point, nil);
            let view_rect = NSView::frame(view);
            Vec2{x:view_point.x as f32, y:view_rect.size.height as f32 - view_point.y as f32}
        }
    }

    fn mouse_motion(this: &Object, event: id) {
        let cw = get_cocoa_window(this);
        let pos = mouse_pos_from_event(this, event);
        let modifiers = get_event_key_modifier(event);
        cw.send_finger_hover_and_move(pos, modifiers);
    }

    extern fn mouse_moved(this: &Object, _sel: Sel, event: id) {
        mouse_motion(this, event);
    }

    extern fn mouse_dragged(this: &Object, _sel: Sel, event: id) {
        mouse_motion(this, event);
    }

    extern fn right_mouse_dragged(this: &Object, _sel: Sel, event: id) {
        mouse_motion(this, event);
    }

    extern fn other_mouse_dragged(this: &Object, _sel: Sel, event: id) {
        mouse_motion(this, event);
    }

    extern fn draw_rect(this: &Object, _sel: Sel, rect: NSRect) {
        let _cw = get_cocoa_window(this);
        unsafe {
            let superclass = superclass(this);
            let () = msg_send![super(this, superclass), drawRect:rect];
        }
    }

    extern fn reset_cursor_rects(this: &Object, _sel: Sel) {
        let cw = get_cocoa_window(this);
        let current_cursor = cw.current_cursor.clone();
        let cursor_id =  *cw.cursors.entry(current_cursor.clone()).or_insert_with(||{
           load_mouse_cursor(current_cursor.clone())
        });
        unsafe {
            let bounds: NSRect = msg_send![this, bounds];
            let _: () = msg_send![this,
              addCursorRect:bounds
              cursor:cursor_id
            ];
        }
    }
    
    // NSTextInput protocol
    extern fn marked_range(this: &Object, _sel: Sel) -> NSRange {
        //println!("markedRange");
        unsafe {
            let marked_text: id = *this.get_ivar("markedText");
            let length = marked_text.length();
            if length > 0 {
                NSRange::new(0, length - 1)
            } else {
                NSRange {
                    location: NSInteger::max_value() as NSUInteger,
                    length: 0,
                }
            }
        }
    }

    extern fn selected_range(_this: &Object, _sel: Sel) -> NSRange {
        NSRange {
            location: 0,
            length: 1,
        }
    }

    extern fn has_marked_text(this: &Object, _sel: Sel) -> BOOL {
        unsafe {
            let marked_text: id = *this.get_ivar("markedText");
            (marked_text.length() > 0) as i8
        }
    }

    extern fn set_marked_text(
        this: &mut Object,
        _sel: Sel,
        string: id,
        _selected_range: NSRange,
        _replacement_range: NSRange,
    ) {
        unsafe {
            let marked_text_ref: &mut id = this.get_mut_ivar("markedText");
            let _: () = msg_send![(*marked_text_ref), release];
            let marked_text = NSMutableAttributedString::alloc(nil);
            let has_attr = msg_send![string, isKindOfClass:class!(NSAttributedString)];
            if has_attr {
                marked_text.init_with_attributed_string(string);
            } else {
                marked_text.init_with_string(string);
            };
            *marked_text_ref = marked_text;
        }
    }

    extern fn unmark_text(this: &Object, _sel: Sel) {
        let cw = get_cocoa_window(this);
        unsafe {
            let marked_text: id = *this.get_ivar("markedText");
            let mutable_string = marked_text.mutable_string();
            let _: () = msg_send![mutable_string, setString:cw.empty_string.unwrap()];
            let input_context: id = msg_send![this, inputContext];
            let _: () = msg_send![input_context, discardMarkedText];
        }    
    }

    extern fn valid_attributes_for_marked_text(this: &Object, _sel: Sel) -> id {
        let cw = get_cocoa_window(this);
        cw.attributes_for_marked_text.unwrap()
    }

    extern fn attributed_substring_for_proposed_range(
        _this: &Object,
        _sel: Sel,
        _range: NSRange,
        _actual_range: *mut c_void, // *mut NSRange
    ) -> id {
        nil
    }

    extern fn character_index_for_point(_this: &Object, _sel: Sel, _point: NSPoint) -> NSUInteger {
       // println!("character_index_for_point");
        0
    }

    extern fn first_rect_for_character_range(this: &Object, _sel: Sel, _range: NSRange, _actual_range: *mut c_void) -> NSRect {
        let cw = get_cocoa_window(this);

        let view: id = this as *const _ as *mut _;
        //let window_point = event.locationInWindow();
        //et view_point = view.convertPoint_fromView_(window_point, nil);
        let view_rect = unsafe{NSView::frame(view)};
        let window_rect = unsafe{NSWindow::frame(cw.window.unwrap())};

        let origin = cw.get_position();
        let bar = (window_rect.size.height - view_rect.size.height) as f32 - 5.;
        NSRect::new(
            NSPoint::new((origin.x + cw.ime_spot.x) as f64, (origin.y + (view_rect.size.height as f32 - cw.ime_spot.y - bar)) as f64),// as _, y as _),
            NSSize::new(0.0, 0.0),
        )
    }

    extern fn insert_text(this: &Object, _sel: Sel, string: id, replacement_range: NSRange) {
        let cw = get_cocoa_window(this);
        unsafe{
            let has_attr = msg_send![string, isKindOfClass:class!(NSAttributedString)];
            let characters = if has_attr {
                msg_send![string, string]
            } else {
                string
            };
            let string = nsstring_to_string(characters);
            cw.send_text_input(string, replacement_range.length != 0);
            let input_context: id = msg_send![this, inputContext];
            msg_send![input_context, invalidateCharacterCoordinates];
            msg_send![cw.view.unwrap(), setNeedsDisplay:YES];
            unmark_text(this, _sel);
           // msg_send![cw.view.unwrap(), unmarkText];
        }
    }

    extern fn do_command_by_selector(this: &Object, _sel: Sel, _command: Sel) {
        let _cw = get_cocoa_window(this);
    }

    extern fn key_down(this: &Object, _sel: Sel, event: id) {
        let _cw = get_cocoa_window(this);
        unsafe{
            let input_context: id = msg_send![this, inputContext];
            msg_send![input_context, handleEvent:event];
            //println!("HERE {} ",ret as u32);
            //let is_repeat:bool = msg_send![event, isARepeat];
            //if !is_repeat{
            //    let array: id = msg_send![class!(NSArray), arrayWithObject:event];
            //    let (): _ = msg_send![this, interpretKeyEvents:array];
           // }
        }
    }

    extern fn key_up(_this: &Object, _sel: Sel, _event: id) {
    }

    extern fn insert_tab(this: &Object, _sel: Sel, _sender: id) {
        unsafe {
            let window: id = msg_send![this, window];
            let first_responder: id = msg_send![window, firstResponder];
            let this_ptr = this as *const _ as *mut _;
            if first_responder == this_ptr {
                let (): _ = msg_send![window, selectNextKeyView:this];
            }
        }
    }

    extern fn insert_back_tab(this: &Object, _sel: Sel, _sender: id) {
        unsafe {
            let window: id = msg_send![this, window];
            let first_responder: id = msg_send![window, firstResponder];
            let this_ptr = this as *const _ as *mut _;
            if first_responder == this_ptr {
                let (): _ = msg_send![window, selectPreviousKeyView:this];
            }
        }
    }

    extern fn yes_function(_this: &Object, _se: Sel, _event: id) -> BOOL {
        YES
    }

    let superclass = class!(NSView);
    let mut decl = ClassDecl::new("RenderViewClass", superclass).unwrap();
    unsafe{
        decl.add_method(sel!(dealloc), dealloc as extern fn(&Object, Sel));
        decl.add_method(sel!(initWithPtr:), init_with_ptr as extern fn(&Object, Sel, *mut c_void) -> id);
        decl.add_method(sel!(drawRect:), draw_rect as extern fn(&Object, Sel, NSRect));
        decl.add_method(sel!(resetCursorRects), reset_cursor_rects as extern fn(&Object, Sel));
        decl.add_method(sel!(hasMarkedText), has_marked_text as extern fn(&Object, Sel) -> BOOL);
        decl.add_method(sel!(markedRange), marked_range as extern fn(&Object, Sel) -> NSRange);
        decl.add_method(sel!(selectedRange), selected_range as extern fn(&Object, Sel) -> NSRange);
        decl.add_method(sel!(setMarkedText:selectedRange:replacementRange:),set_marked_text as extern fn(&mut Object, Sel, id, NSRange, NSRange));
        decl.add_method(sel!(unmarkText), unmark_text as extern fn(&Object, Sel));
        decl.add_method(sel!(validAttributesForMarkedText),valid_attributes_for_marked_text as extern fn(&Object, Sel) -> id);
        decl.add_method(
            sel!(attributedSubstringForProposedRange:actualRange:),
            attributed_substring_for_proposed_range
                as extern fn(&Object, Sel, NSRange, *mut c_void) -> id,
        );
        decl.add_method(
            sel!(insertText:replacementRange:),
            insert_text as extern fn(&Object, Sel, id, NSRange),
        );
        decl.add_method(
            sel!(characterIndexForPoint:),
            character_index_for_point as extern fn(&Object, Sel, NSPoint) -> NSUInteger,
        );
        decl.add_method(
            sel!(firstRectForCharacterRange:actualRange:),
            first_rect_for_character_range
                as extern fn(&Object, Sel, NSRange, *mut c_void) -> NSRect,
        );
        decl.add_method(sel!(doCommandBySelector:),do_command_by_selector as extern fn(&Object, Sel, Sel));
        decl.add_method(sel!(keyDown:), key_down as extern fn(&Object, Sel, id));
        decl.add_method(sel!(keyUp:), key_up as extern fn(&Object, Sel, id));
        //decl.add_method(sel!(insertTab:), insert_tab as extern fn(&Object, Sel, id));
        //decl.add_method(sel!(insertBackTab:), insert_back_tab as extern fn(&Object, Sel, id));
        decl.add_method(sel!(mouseDown:), mouse_down as extern fn(&Object, Sel, id));
        decl.add_method(sel!(mouseUp:), mouse_up as extern fn(&Object, Sel, id));
        decl.add_method(sel!(rightMouseDown:), right_mouse_down as extern fn(&Object, Sel, id));
        decl.add_method(sel!(rightMouseUp:), right_mouse_up as extern fn(&Object, Sel, id));
        decl.add_method(sel!(otherMouseDown:), other_mouse_down as extern fn(&Object, Sel, id));
        decl.add_method(sel!(otherMouseUp:), other_mouse_up as extern fn(&Object, Sel, id));
        decl.add_method(sel!(mouseMoved:), mouse_moved as extern fn(&Object, Sel, id));
        decl.add_method(sel!(mouseDragged:), mouse_dragged as extern fn(&Object, Sel, id));
        decl.add_method(sel!(rightMouseDragged:), right_mouse_dragged as extern fn(&Object, Sel, id));
        decl.add_method(sel!(otherMouseDragged:), other_mouse_dragged as extern fn(&Object, Sel, id));
        decl.add_method(sel!(wantsKeyDownForEvent:), yes_function as extern fn(&Object, Sel, id) -> BOOL);
        decl.add_method(sel!(acceptsFirstResponder:), yes_function as extern fn(&Object, Sel, id) -> BOOL);
        decl.add_method(sel!(becomeFirstResponder:), yes_function as extern fn(&Object, Sel, id) -> BOOL);
        decl.add_method(sel!(resignFirstResponder:), yes_function as extern fn(&Object, Sel, id) -> BOOL);
    }
    decl.add_ivar::<*mut c_void>("cocoa_window_ptr");
    decl.add_ivar::<id>("markedText");
    let protocol = Protocol::get("NSTextInputClient").unwrap();
    //println!("NTextInputClient {}", protocol);
    decl.add_protocol(&protocol);

    return decl.register();
}

pub unsafe fn superclass<'a>(this: &'a Object) -> &'a Class {
    let superclass: id = msg_send![this, superclass];
    &*(superclass as *const _)
}

pub fn bottom_left_to_top_left(rect: NSRect) -> f64 {
    CGDisplay::main().pixels_high() as f64 - (rect.origin.y + rect.size.height)
}

fn load_mouse_cursor(cursor:MouseCursor)->id{
    match cursor {
        MouseCursor::Arrow | MouseCursor::Default | MouseCursor::Hidden => load_native_cursor("arrowCursor"),
        MouseCursor::Hand => load_native_cursor("pointingHandCursor"),
        MouseCursor::Grabbing | MouseCursor::Grab => load_native_cursor("closedHandCursor"),
        MouseCursor::Text => load_native_cursor("IBeamCursor"),
        MouseCursor::VerticalText => load_native_cursor("IBeamCursorForVerticalLayout"),
        MouseCursor::Copy => load_native_cursor("dragCopyCursor"),
        MouseCursor::Alias => load_native_cursor("dragLinkCursor"),
        MouseCursor::NotAllowed | MouseCursor::NoDrop => load_native_cursor("operationNotAllowedCursor"),
        MouseCursor::ContextMenu => load_native_cursor("contextualMenuCursor"),
        MouseCursor::Crosshair => load_native_cursor("crosshairCursor"),
        MouseCursor::EResize => load_native_cursor("resizeRightCursor"),
        MouseCursor::NResize => load_native_cursor("resizeUpCursor"),
        MouseCursor::WResize => load_native_cursor("resizeLeftCursor"),
        MouseCursor::SResize => load_native_cursor("resizeDownCursor"),
        MouseCursor::EwResize | MouseCursor::ColResize => load_native_cursor("resizeLeftRightCursor"),
        MouseCursor::NsResize | MouseCursor::RowResize => load_native_cursor("resizeUpDownCursor"),

        // Undocumented cursors: https://stackoverflow.com/a/46635398/5435443
        MouseCursor::Help => load_undocumented_cursor("_helpCursor"),
        MouseCursor::ZoomIn => load_undocumented_cursor("_zoomInCursor"),
        MouseCursor::ZoomOut => load_undocumented_cursor("_zoomOutCursor"),
        MouseCursor::NeResize => load_undocumented_cursor("_windowResizeNorthEastCursor"),
        MouseCursor::NwResize => load_undocumented_cursor("_windowResizeNorthWestCursor"),
        MouseCursor::SeResize => load_undocumented_cursor("_windowResizeSouthEastCursor"),
        MouseCursor::SwResize => load_undocumented_cursor("_windowResizeSouthWestCursor"),
        MouseCursor::NeswResize => load_undocumented_cursor("_windowResizeNorthEastSouthWestCursor"),
        MouseCursor::NwseResize => load_undocumented_cursor("_windowResizeNorthWestSouthEastCursor"),

        // While these are available, the former just loads a white arrow,
        // and the latter loads an ugly deflated beachball!
        // MouseCursor::Move => Cursor::Undocumented("_moveCursor"),
        // MouseCursor::Wait => Cursor::Undocumented("_waitCursor"),
        // An even more undocumented cursor...
        // https://bugs.eclipse.org/bugs/show_bug.cgi?id=522349
        // This is the wrong semantics for `Wait`, but it's the same as
        // what's used in Safari and Chrome.
        MouseCursor::Wait | MouseCursor::Progress => load_undocumented_cursor("busyButClickableCursor"),

        // For the rest, we can just snatch the cursors from WebKit...
        // They fit the style of the native cursors, and will seem
        // completely standard to macOS users.
        // https://stackoverflow.com/a/21786835/5435443
        MouseCursor::Move | MouseCursor::AllScroll => load_webkit_cursor("move"),
        MouseCursor::Cell => load_webkit_cursor("cell"),
    }
}

fn load_native_cursor(cursor_name:&str)->id{
    let sel = Sel::register(cursor_name);
    let id:id = unsafe{msg_send![class!(NSCursor), performSelector:sel]};
    id
}

fn load_undocumented_cursor(cursor_name:&str)->id{
    unsafe{
        let class = class!(NSCursor);
        let sel = Sel::register(cursor_name);
        let sel = msg_send![class, respondsToSelector:sel];
        let id:id = msg_send![class, performSelector:sel];
        id
    }
}

fn load_webkit_cursor(cursor_name_str: &str) -> id {
    unsafe{
        static CURSOR_ROOT: &'static str = "/System/Library/Frameworks/ApplicationServices.framework/Versions/A/Frameworks/HIServices.framework/Versions/A/Resources/cursors";
        let cursor_root = NSString::alloc(nil).init_str(CURSOR_ROOT);
        let cursor_name = NSString::alloc(nil).init_str(cursor_name_str);
        let cursor_pdf = NSString::alloc(nil).init_str("cursor.pdf");
        let cursor_plist = NSString::alloc(nil).init_str("info.plist");
        let key_x = NSString::alloc(nil).init_str("hotx");
        let key_y = NSString::alloc(nil).init_str("hoty");

        let cursor_path: id = msg_send![cursor_root,
            stringByAppendingPathComponent:cursor_name
        ];
        let pdf_path: id = msg_send![cursor_path,
            stringByAppendingPathComponent:cursor_pdf
        ];
        let info_path: id = msg_send![cursor_path,
            stringByAppendingPathComponent:cursor_plist
        ];

        let image = NSImage::alloc(nil).initByReferencingFile_(pdf_path);
        let info = NSDictionary::dictionaryWithContentsOfFile_(nil, info_path);

        let x = info.valueForKey_(key_x);
        let y = info.valueForKey_(key_y);
        let point = NSPoint::new(
            msg_send![x, doubleValue],
            msg_send![y, doubleValue],
        );
        let cursor: id = msg_send![class!(NSCursor), alloc];
        msg_send![cursor, initWithImage:image hotSpot:point]
    }
}

unsafe fn nsstring_to_string(string:id)->String{
    let slice = std::slice::from_raw_parts(
        string.UTF8String() as *const std::os::raw::c_uchar,
        string.len(),
    );
    std::str::from_utf8_unchecked(slice).to_owned()
}




#[repr(C)]
pub struct NSRange {
    pub location: NSUInteger,
    pub length: NSUInteger,
}

impl NSRange {
    #[inline]
    pub fn new(location: NSUInteger, length: NSUInteger) -> NSRange {
        NSRange { location, length }
    }
}

unsafe impl objc::Encode for NSRange {
    fn encode() -> objc::Encoding {
        let encoding = format!(
            // TODO: Verify that this is correct
            "{{NSRange={}{}}}",
            NSUInteger::encode().as_str(),
            NSUInteger::encode().as_str(),
        );
        unsafe { objc::Encoding::from_str(&encoding) }
    }
}

pub trait NSMutableAttributedString: Sized {
    unsafe fn alloc(_: Self) -> id {
        msg_send![class!(NSMutableAttributedString), alloc]
    }

    unsafe fn init(self) -> id; // *mut NSMutableAttributedString
    unsafe fn init_with_string(self, string: id) -> id;
    unsafe fn init_with_attributed_string(self, string: id) -> id;

    unsafe fn string(self) -> id; // *mut NSString
    unsafe fn mutable_string(self) -> id; // *mut NSMutableString
    unsafe fn length(self) -> NSUInteger;
}

impl NSMutableAttributedString for id {
    unsafe fn init(self) -> id {
        msg_send![self, init]
    }

    unsafe fn init_with_string(self, string: id) -> id {
        msg_send![self, initWithString:string]
    }

    unsafe fn init_with_attributed_string(self, string: id) -> id {
        msg_send![self, initWithAttributedString:string]
    }

    unsafe fn string(self) -> id {
        msg_send![self, string]
    }

    unsafe fn mutable_string(self) -> id {
        msg_send![self, mutableString]
    }

    unsafe fn length(self) -> NSUInteger {
        msg_send![self, length]
    }
}