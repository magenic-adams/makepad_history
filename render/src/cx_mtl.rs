use std::mem;

//use cocoa::base::{id};
use cocoa::appkit::{NSWindow, NSView};
use cocoa::foundation::{NSAutoreleasePool, NSUInteger, NSRange};
use core_graphics::geometry::CGSize;
use objc::{msg_send, sel, sel_impl};
use objc::runtime::YES;
use metal::*;
use crate::cx_cocoa::*;
use crate::cx::*;

impl Cx {
     
    pub fn exec_draw_list(&mut self, draw_list_id: usize, device: &Device, encoder: &RenderCommandEncoderRef) {
        
        // tad ugly otherwise the borrow checker locks 'self' and we can't recur
        let draw_calls_len = self.draw_lists[draw_list_id].draw_calls_len;
        for draw_call_id in 0..draw_calls_len {
            let sub_list_id = self.draw_lists[draw_list_id].draw_calls[draw_call_id].sub_list_id;
            if sub_list_id != 0 {
                self.exec_draw_list(sub_list_id, device, encoder);
            }
            else {
                let draw_list = &mut self.draw_lists[draw_list_id];
                draw_list.set_clipping_uniforms();
                draw_list.platform.uni_dl.update_with_f32_data(device, &draw_list.uniforms);
                let draw_call = &mut draw_list.draw_calls[draw_call_id];
                let sh = &self.shaders[draw_call.shader_id];
                let shc = &self.compiled_shaders[draw_call.shader_id];
                
                if draw_call.instance_dirty {
                    draw_call.instance_dirty = false;
                    // update the instance buffer data
                    draw_call.platform.inst_vbuf.update_with_f32_data(device, &draw_call.instance);
                }
                if draw_call.uniforms_dirty {
                    draw_call.uniforms_dirty = false;
                    draw_call.platform.uni_dr.update_with_f32_data(device, &draw_call.uniforms);
                }
                
                // lets verify our instance_offset is not disaligned
                let instances = (draw_call.instance.len() / shc.instance_slots) as u64;
                if let Some(pipeline_state) = &shc.pipeline_state {
                    encoder.set_render_pipeline_state(pipeline_state);
                    if let Some(buf) = &shc.geom_vbuf.multi_buffer_read().buffer {encoder.set_vertex_buffer(0, Some(&buf), 0);}
                    else {println!("Drawing error: geom_vbuf None")}
                    if let Some(buf) = &draw_call.platform.inst_vbuf.multi_buffer_read().buffer {encoder.set_vertex_buffer(1, Some(&buf), 0);}
                    else {println!("Drawing error: inst_vbuf None")}
                    if let Some(buf) = &self.platform.uni_cx.multi_buffer_read().buffer {encoder.set_vertex_buffer(2, Some(&buf), 0);}
                    else {println!("Drawing error: uni_cx None")}
                    if let Some(buf) = &draw_list.platform.uni_dl.multi_buffer_read().buffer {encoder.set_vertex_buffer(3, Some(&buf), 0);}
                    else {println!("Drawing error: uni_dl None")}
                    if let Some(buf) = &draw_call.platform.uni_dr.multi_buffer_read().buffer {encoder.set_vertex_buffer(4, Some(&buf), 0);}
                    else {println!("Drawing error: uni_dr None")}
                    
                    if let Some(buf) = &self.platform.uni_cx.multi_buffer_read().buffer {encoder.set_fragment_buffer(0, Some(&buf), 0);}
                    else {println!("Drawing error: uni_cx None")}
                    if let Some(buf) = &draw_list.platform.uni_dl.multi_buffer_read().buffer {encoder.set_fragment_buffer(1, Some(&buf), 0);}
                    else {println!("Drawing error: uni_dl None")}
                    if let Some(buf) = &draw_call.platform.uni_dr.multi_buffer_read().buffer {encoder.set_fragment_buffer(2, Some(&buf), 0);}
                    else {println!("Drawing error: uni_dr None")}
                    // lets set our textures
                    for (i, texture_id) in draw_call.textures_2d.iter().enumerate() {
                        let tex = &mut self.textures_2d[*texture_id as usize];
                        if tex.dirty {
                            tex.upload_to_device(device);
                        }
                        if let Some(mtltex) = &tex.mtltexture {
                            encoder.set_fragment_texture(i as NSUInteger, Some(&mtltex));
                            encoder.set_vertex_texture(i as NSUInteger, Some(&mtltex));
                        }
                    }
                    
                    if let Some(buf) = &shc.geom_ibuf.multi_buffer_read().buffer {
                        encoder.draw_indexed_primitives_instanced(
                            MTLPrimitiveType::Triangle,
                            sh.geometry_indices.len() as u64,
                            // Index Count
                            MTLIndexType::UInt32,
                            // indexType,
                            &buf,
                            // index buffer
                            0,
                            // index buffer offset
                            instances,
                            // instance count
                        )
                    }
                    else {println!("Drawing error: geom_ibuf None")}
                }
            }
        }
    }
    
    pub fn repaint(&mut self, layer: &CoreAnimationLayer, device: &Device, command_queue: &CommandQueue, wait: bool) {
        let pool = unsafe {NSAutoreleasePool::new(cocoa::base::nil)};
        //let command_buffer = command_queue.new_command_buffer();
        if let Some(drawable) = layer.next_drawable() {
            self.prepare_frame();
            
            let render_pass_descriptor = RenderPassDescriptor::new();
            
            let color_attachment = render_pass_descriptor.color_attachments().object_at(0).unwrap();
            color_attachment.set_texture(Some(drawable.texture()));
            color_attachment.set_load_action(MTLLoadAction::Clear);
            color_attachment.set_clear_color(MTLClearColor::new(
                self.clear_color.r as f64,
                self.clear_color.g as f64,
                self.clear_color.b as f64,
                self.clear_color.a as f64
            ));
            color_attachment.set_store_action(MTLStoreAction::Store);
            
            let command_buffer = command_queue.new_command_buffer();
            
            render_pass_descriptor.color_attachments().object_at(0).unwrap().set_load_action(MTLLoadAction::Clear);
            
            let encoder = command_buffer.new_render_command_encoder(&render_pass_descriptor);
            //let encoder = parallel_encoder.render_command_encoder();
            
            self.platform.uni_cx.update_with_f32_data(&device, &self.uniforms);
            
            // ok now we should call our render thing
            self.exec_draw_list(0, &device, encoder);
            /*
            match &self.debug_area{
                Area::All=>self.debug_draw_tree_recur(0, 0),
                Area::Instance(ia)=>self.debug_draw_tree_recur(ia.draw_list_id, 0),
                Area::DrawList(dl)=>self.debug_draw_tree_recur(dl.draw_list_id, 0),
                _=>()
            }*/
            
            encoder.end_encoding();
            //parallel_encoder.end_encoding();
            
            command_buffer.present_drawable(&drawable);
            command_buffer.commit();
            
            if wait {
                command_buffer.wait_until_completed();
            }
        }
        unsafe {
            msg_send![pool, release];
        }
    }
    
    fn resize_layer_to_turtle(&mut self, layer: &CoreAnimationLayer) {
        layer.set_drawable_size(CGSize::new(
            (self.window_geom.inner_size.x * self.window_geom.dpi_factor) as f64,
            (self.window_geom.inner_size.y * self.window_geom.dpi_factor) as f64
        ));
    }
    
    pub fn event_loop<F>(&mut self, mut event_handler: F)
    where F: FnMut(&mut Cx, &mut Event),
    {
        self.feature = "mtl".to_string();
        CocoaWindow::cocoa_app_init();
        
        let mut cocoa_window = CocoaWindow {..Default::default()};
        
        cocoa_window.init(&self.title);
        
        let device = Device::system_default();
        
        let layer = CoreAnimationLayer::new();
        layer.set_device(&device);
        layer.set_pixel_format(MTLPixelFormat::BGRA8Unorm);
        layer.set_presents_with_transaction(false);
        
        unsafe {
            //msg_send![layer, displaySyncEnabled:false];
            let count: u64 = 2;
            msg_send![layer, setMaximumDrawableCount: count];
            msg_send![layer, setDisplaySyncEnabled: true];
        }
        
        unsafe {
            let view = cocoa_window.window.unwrap().contentView();
            view.setWantsBestResolutionOpenGLSurface_(YES);
            view.setWantsLayer(YES);
            view.setLayer(mem::transmute(layer.as_ref()));
        }
        
        // ok get_inner_size eh. lets do this
        
        self.window_geom = cocoa_window.get_window_geom();
        
        layer.set_drawable_size(CGSize::new(
            (self.window_geom.inner_size.x * self.window_geom.dpi_factor) as f64,
            (self.window_geom.inner_size.y * self.window_geom.dpi_factor) as f64
        ));
        
        let command_queue = device.new_command_queue();
        
        let mut root_view = View::<NoScrollBar> {
            ..Style::style(self)
        };
        
        // move it to my second screen. livecompile.
        //cocoa_window.set_position(Vec2 {x: 1920.0, y: 400.0});
        
        self.mtl_compile_all_shaders(&device);
        
        self.load_binary_deps_from_file();
        
        self.call_event_handler(&mut event_handler, &mut Event::Construct);
        
        self.redraw_area(Area::All);
        
        while self.running {
            //println!("{}{} ",self.playing_anim_areas.len(), self.redraw_areas.len());
            cocoa_window.poll_events(
                self.playing_anim_areas.len() == 0 && self.redraw_areas.len() == 0 && self.next_frame_callbacks.len() == 0,
                | events | {
                    for mut event in events {
                        match &mut event {
                            Event::FingerHover(_) => {
                                self.hover_mouse_cursor = None;
                            },
                            Event::FingerUp(_) => {
                                self.down_mouse_cursor = None;
                            },
                            Event::CloseRequested => {
                                self.running = false
                            },
                            Event::FingerDown(fe) => {
                                // lets set the finger tap count
                                fe.tap_count = self.process_tap_count(fe.digit, fe.abs, fe.time);
                            },
                            Event::KeyDown(ke) => {
                                self.process_key_down(ke.clone());
                                if ke.key_code == KeyCode::PrintScreen {
                                    if ke.modifiers.control {
                                        self.panic_redraw = true;
                                    }
                                    else {
                                        self.panic_now = true;
                                    }
                                }
                            },
                            Event::KeyUp(ke) => {
                                self.process_key_up(&ke);
                            },
                            Event::AppFocusLost => {
                                self.call_all_keys_up(&mut event_handler);
                            },
                            _ => ()
                        };
                        match &event {
                            Event::WindowChange(re) => { // do this here because mac
                                self.window_geom = re.new_geom.clone();
                                
                                self.call_event_handler(&mut event_handler, &mut event);
                                
                                self.redraw_area(Area::All);
                                self.call_draw_event(&mut event_handler, &mut root_view);
                                
                                self.repaint(&layer, &device, &command_queue, false);
                                self.resize_layer_to_turtle(&layer);
                            },
                            Event::None => {
                            },
                            _ => {
                                //let time_now = precise_time_ns();
                                self.call_event_handler(&mut event_handler, &mut event);
                                
                                //let time_now_next = precise_time_ns();
                                //println!("Animation took: {}", ((time_now_next - time_now) as f64) / 1_000_000_000.0);
                            }
                        }
                        match &event {
                            Event::FingerUp(fe) => { // decapture automatically
                                self.captured_fingers[fe.digit] = Area::Empty;
                            },
                            _ => {}
                        }
                    }
                }
            );
            
            if self.playing_anim_areas.len() != 0 {
                let time = cocoa_window.time_now();
                // keeps the error as low as possible
                self.call_animation_event(&mut event_handler, time);
            }
            
            if self.next_frame_callbacks.len() != 0 {
                let time = cocoa_window.time_now();
                // keeps the error as low as possible
                self.call_frame_event(&mut event_handler, time);
            }
            
            self.call_signals_before_draw(&mut event_handler);
            
            // call redraw event
            if self.redraw_areas.len()>0 {
                let time_start = cocoa_window.time_now();
                self.call_draw_event(&mut event_handler, &mut root_view);
                self.paint_dirty = true;
                let time_end = cocoa_window.time_now();
                println!("Redraw took: {}", (time_end - time_start));
            }
            
            self.process_desktop_file_read_requests(&mut event_handler);
            
            self.call_signals_after_draw(&mut event_handler);
            
            // set a cursor
            if !self.down_mouse_cursor.is_none() {
                cocoa_window.set_mouse_cursor(self.down_mouse_cursor.as_ref().unwrap().clone())
            }
            else if !self.hover_mouse_cursor.is_none() {
                cocoa_window.set_mouse_cursor(self.hover_mouse_cursor.as_ref().unwrap().clone())
            }
            else {
                cocoa_window.set_mouse_cursor(MouseCursor::Default)
            }
            
            if let Some(set_ime_position) = self.platform.set_ime_position {
                self.platform.set_ime_position = None;
                cocoa_window.ime_spot = set_ime_position;
            }
            
            if let Some(window_position) = self.platform.set_window_position {
                self.platform.set_window_position = None;
                cocoa_window.set_position(window_position);
                self.window_geom = cocoa_window.get_window_geom();
            }
            
            if let Some(window_outer_size) = self.platform.set_window_outer_size {
                self.platform.set_window_outer_size = None;
                cocoa_window.set_outer_size(window_outer_size);
                self.window_geom = cocoa_window.get_window_geom();
                self.resize_layer_to_turtle(&layer);
            }
            
            while self.platform.start_timer.len()>0 {
                let (timer_id, interval, repeats) = self.platform.start_timer.pop().unwrap();
                cocoa_window.start_timer(timer_id, interval, repeats);
            }
            
            while self.platform.stop_timer.len()>0 {
                let timer_id = self.platform.stop_timer.pop().unwrap();
                cocoa_window.stop_timer(timer_id);
            }
            
            // repaint everything if we need to
            if self.paint_dirty {
                self.paint_dirty = false;
                self.repaint_id += 1;
                self.repaint(&layer, &device, &command_queue, true);
            }
        }
    }
    
    pub fn show_text_ime(&mut self, x: f32, y: f32) {
        self.platform.set_ime_position = Some(Vec2 {x: x, y: y});
    }
    
    pub fn hide_text_ime(&mut self) {
    }
    
    pub fn set_window_outer_size(&mut self, size: Vec2) {
        self.platform.set_window_outer_size = Some(size);
    }
    
    pub fn set_window_position(&mut self, pos: Vec2) {
        self.platform.set_window_position = Some(pos);
    }
    
    pub fn start_timer(&mut self, interval: f64, repeats: bool) -> Timer {
        self.timer_id += 1;
        self.platform.start_timer.push((self.timer_id, interval, repeats));
        Timer{timer_id:self.timer_id}
    }  
     
    pub fn stop_timer(&mut self, timer:&mut Timer) {
        if timer.timer_id != 0{
            self.platform.stop_timer.push(timer.timer_id);
            timer.timer_id = 0;
        }
    }
    
    pub fn send_signal(signal: Signal, value: u64) {
        CocoaWindow::post_signal(signal.signal_id, value);
    }
   
   
    //pub fn send_custom_event_before_draw(&mut self, id:u64, message:u64){
    //   self.custom_before_draw.push((id, message));
    //}
   
}

#[derive(Clone, Default)]
pub struct CxPlatform {
    pub uni_cx: MetalBuffer,
    pub post_id: u64,
    pub set_window_position: Option<Vec2>,
    pub set_window_outer_size: Option<Vec2>,
    pub set_ime_position: Option<Vec2>,
    pub start_timer: Vec<(u64, f64, bool)>,
    pub stop_timer: Vec<(u64)>,
    pub text_clipboard_response: Option<String>,
    pub desktop: CxDesktop,
}

#[derive(Clone, Default)]
pub struct DrawListPlatform {
    pub uni_dl: MetalBuffer
}

#[derive(Default, Clone, Debug)]
pub struct DrawCallPlatform {
    pub uni_dr: MetalBuffer,
    pub inst_vbuf: MetalBuffer
}

#[derive(Default, Clone, Debug)]
pub struct MultiMetalBuffer {
    pub buffer: Option<metal::Buffer>,
    pub size: usize,
    pub used: usize
}

#[derive(Default, Clone, Debug)]
pub struct MetalBuffer {
    pub last_written: usize,
    pub multi1: MultiMetalBuffer,
    pub multi2: MultiMetalBuffer,
    pub multi3: MultiMetalBuffer,
    pub multi4: MultiMetalBuffer,
    pub multi5: MultiMetalBuffer,
    pub multi6: MultiMetalBuffer,
}

impl MetalBuffer {
    pub fn multi_buffer_read(&self) -> &MultiMetalBuffer {
        match self.last_written {
            0 => &self.multi1,
            1 => &self.multi2,
            2 => &self.multi3,
            3 => &self.multi4,
            4 => &self.multi5,
            _ => &self.multi6,
        }
    }
    
    pub fn multi_buffer_write(&mut self) -> &mut MultiMetalBuffer {
        self.last_written = (self.last_written + 1) % 3;
        match self.last_written {
            0 => &mut self.multi1,
            1 => &mut self.multi2,
            2 => &mut self.multi3,
            3 => &mut self.multi4,
            4 => &mut self.multi5,
            _ => &mut self.multi6,
        }
    }
    
    pub fn update_with_f32_data(&mut self, device: &Device, data: &Vec<f32>) {
        let elem = self.multi_buffer_write();
        if elem.size < data.len() {
            elem.buffer = None;
        }
        if let None = &elem.buffer {
            elem.buffer = Some(
                device.new_buffer(
                    (data.len() * std::mem::size_of::<f32>()) as u64,
                    MTLResourceOptions::CPUCacheModeDefaultCache
                )
            );
            elem.size = data.len()
        }
        if let Some(buffer) = &elem.buffer {
            let p = buffer.contents();
            
            unsafe {
                std::ptr::copy(data.as_ptr(), p as *mut f32, data.len());
            }
            buffer.did_modify_range(NSRange::new(0 as u64, (data.len() * std::mem::size_of::<f32>()) as u64));
        }
        elem.used = data.len()
    }
    
    pub fn update_with_u32_data(&mut self, device: &Device, data: &Vec<u32>) {
        let elem = self.multi_buffer_write();
        if elem.size < data.len() {
            elem.buffer = None;
        }
        if let None = &elem.buffer {
            elem.buffer = Some(
                device.new_buffer(
                    (data.len() * std::mem::size_of::<u32>()) as u64,
                    MTLResourceOptions::CPUCacheModeDefaultCache
                )
            );
            elem.size = data.len()
        }
        if let Some(buffer) = &elem.buffer {
            let p = buffer.contents();
            
            unsafe {
                std::ptr::copy(data.as_ptr(), p as *mut u32, data.len());
            }
            buffer.did_modify_range(NSRange::new(0 as u64, (data.len() * std::mem::size_of::<u32>()) as u64));
        }
        elem.used = data.len()
    }
}


#[derive(Default, Clone)]
pub struct Texture2D {
    pub texture_id: usize,
    pub dirty: bool,
    pub image: Vec<u32>,
    pub width: usize,
    pub height: usize,
    pub mtltexture: Option<metal::Texture>
}

impl Texture2D {
    pub fn resize(&mut self, width: usize, height: usize) {
        self.width = width;
        self.height = height;
        self.image.resize((width * height) as usize, 0);
        self.dirty = true;
    }
    
    pub fn upload_to_device(&mut self, device: &Device) {
        let desc = TextureDescriptor::new();
        desc.set_texture_type(MTLTextureType::D2);
        desc.set_pixel_format(MTLPixelFormat::BGRA8Unorm);
        desc.set_width(self.width as u64);
        desc.set_height(self.height as u64);
        desc.set_storage_mode(MTLStorageMode::Managed);
        //desc.set_mipmap_level_count(1);
        //desc.set_depth(1);
        //desc.set_sample_count(4);
        let tex = device.new_texture(&desc);
        
        let region = MTLRegion {
            origin: MTLOrigin {x: 0, y: 0, z: 0},
            size: MTLSize {width: self.width as u64, height: self.height as u64, depth: 1}
        };
        tex.replace_region(region, 0, (self.width * mem::size_of::<u32>()) as u64, self.image.as_ptr() as *const std::ffi::c_void);
        
        //image_buf.did_modify_range(NSRange::new(0 as u64, (self.image.len() * mem::size_of::<u32>()) as u64));
        
        self.mtltexture = Some(tex);
        self.dirty = false;
        
    }
}