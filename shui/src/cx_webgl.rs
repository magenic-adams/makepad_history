use std::mem;
use std::ptr;

pub use crate::cx_shared::*;
use crate::shader::*;
use crate::cxshaders_gl::*;
use crate::events::*;
use std::alloc;

impl Cx{
     pub fn exec_draw_list(&mut self, draw_list_id: usize){
        // tad ugly otherwise the borrow checker locks 'self' and we can't recur
        let draw_calls_len = self.drawing.draw_lists[draw_list_id].draw_calls_len;

        for draw_call_id in 0..draw_calls_len{

            let sub_list_id = self.drawing.draw_lists[draw_list_id].draw_calls[draw_call_id].sub_list_id;
            if sub_list_id != 0{
                self.exec_draw_list(sub_list_id);
            }
            else{ 
                let draw_list = &mut self.drawing.draw_lists[draw_list_id];
                let draw_call = &mut draw_list.draw_calls[draw_call_id];
                let csh = &self.shaders.compiled_shaders[draw_call.shader_id];

                if draw_call.update_frame_id == self.drawing.frame_id{
                    // update the instance buffer data
                    draw_call.resources.check_attached_vao(csh, &mut self.resources);

                    self.resources.from_wasm.alloc_array_buffer(
                        draw_call.resources.inst_vb_id,
                        draw_call.instance.len(),
                        draw_call.instance.as_ptr() as *const f32
                    );
                }

                // update/alloc textures?
                for tex_id in &draw_call.textures{
                    let tex = &mut self.textures.textures[*tex_id as usize];
                    if tex.dirty{
                        tex.upload_to_device(&mut self.resources);
                    }
                }

                self.resources.from_wasm.draw_call(
                    draw_call.shader_id,
                    draw_call.resources.vao_id,
                    &self.uniforms,
                    self.drawing.frame_id, // update once a frame
                    &draw_list.uniforms,
                    draw_list_id, // update on drawlist change
                    &draw_call.uniforms,
                    draw_call.draw_call_id, // update on drawcall id change
                    &draw_call.textures
                );
            }
        }
    }

    pub fn repaint(&mut self){
        self.resources.from_wasm.clear(self.clear_color.x, self.clear_color.y, self.clear_color.z, self.clear_color.w);
        self.prepare_frame();        
        self.exec_draw_list(0);
    }

    // incoming to_wasm. There is absolutely no other entrypoint
    // to general rust codeflow than this function. Only the allocators and init
    pub fn process_to_wasm<F>(&mut self, msg:u32, mut event_handler:F)->u32
    where F: FnMut(&mut Cx, Event)
    {
        let mut to_wasm = ToWasm::from(msg);
        self.resources.from_wasm = FromWasm::new();
        let mut is_animation_frame = false;
        loop{
            let msg_type = to_wasm.mu32();
            match msg_type{
                0=>{ // end
                    break;
                },
                1=>{ // fetch_deps
                    self.resources.from_wasm.set_document_title(&self.title);
                    // compile all the shaders
                    self.resources.from_wasm.log(&self.title);
                    
                    // send the UI our deps, overlap with shadercompiler
                    let mut load_deps = Vec::new();
                    for font_resource in &self.fonts.font_resources{
                        load_deps.push(font_resource.name.clone());
                    }
                    // other textures, things
                    self.resources.from_wasm.load_deps(load_deps);

                    self.shaders.compile_all_webgl_shaders(&mut self.resources);
                },
                2=>{ // deps_loaded
                    let len = to_wasm.mu32();
                    for _i in 0..len{
                        let name = to_wasm.parse_string();
                        let ptr = to_wasm.mu32();
                        self.binary_deps.push(BinaryDep::new_from_wasm(name, ptr))
                    }
                    
                    // lets load the fonts from binary deps
                    let num_fonts = self.fonts.font_resources.len();
                    for i in 0..num_fonts{
                        let font_file = self.fonts.font_resources[i].name.clone();
                        let bin_dep = self.get_binary_dep(&font_file);
                        if let Some(mut bin_dep) = bin_dep{
                            if let Err(msg) = self.fonts.load_from_binary_dep(&mut bin_dep, &mut self.textures){
                                self.resources.from_wasm.log(&format!("Error loading font! {}", msg));
                            }
                        }
                    }

                },
                3=>{ // init
                    

                    self.turtle.target_size = vec2(to_wasm.mf32(),to_wasm.mf32());
                    self.turtle.target_dpi_factor = to_wasm.mf32();
                    event_handler(self, Event::Init); 
                    self.redraw_all();
                },
                4=>{ // resize
                    self.turtle.target_size = vec2(to_wasm.mf32(),to_wasm.mf32());
                    self.turtle.target_dpi_factor = to_wasm.mf32();
                    
                    // do our initial redraw and repaint
                    self.redraw_all();
                },
                5=>{ // animation_frame
                    is_animation_frame = true;
                    let time = to_wasm.mf64();
                    //log!(self, "{} o clock",time);
                    event_handler(self, Event::Animate(AnimateEvent{time:time}));
                },
                6=>{ // finger messages
                    let finger_event_type = to_wasm.mu32();

                    let finger_event = FingerEvent{
                        x:to_wasm.mf32(),
                        y:to_wasm.mf32(),
                        digit:to_wasm.mu32(),
                        button:to_wasm.mu32(),
                        touch:to_wasm.mu32()>0,
                        x_wheel:to_wasm.mf32(),
                        y_wheel:to_wasm.mf32()
                    };
                    let event = match finger_event_type{
                        1=>Event::FingerDown(finger_event),
                        2=>Event::FingerUp(finger_event),
                        3=>Event::FingerMove(finger_event),
                        4=>Event::FingerHover(finger_event),
                        5=>Event::FingerWheel(finger_event),
                        _=>Event::None
                    };
                    event_handler(self, event);
                },  
                _=>{
                    panic!("Message unknown")
                }
            };
        };

        // if we have to redraw self, do so, 
        if let Some(_) = self.redraw_dirty{
            self.redraw_area = self.redraw_dirty.clone();
            self.redraw_none();
            event_handler(self, Event::Redraw);
            // processing a redraw makes paint dirty by default
            self.paint_dirty = true;
        }
    
        if is_animation_frame && self.paint_dirty{
            self.paint_dirty = false;
            self.repaint();
        }

        // free the received message
        to_wasm.dealloc();
        
        // request animation frame if still need to redraw, or repaint
        // we use request animation frame for that.
        let mut req_anim_frame = false;
        if let Some(_) = self.redraw_dirty{
            req_anim_frame = true
        }
        else if self.animations.len()> 0 || self.paint_dirty{
            req_anim_frame = true
        }
        if req_anim_frame{
            self.resources.from_wasm.request_animation_frame();
        }

        // mark the end of the message
        self.resources.from_wasm.end();

        //return wasm pointer to caller
        self.resources.from_wasm.wasm_ptr()
    }

    // empty stub
    pub fn event_loop<F>(&mut self, mut _event_handler:F)
    where F: FnMut(&mut Cx, Event),
    { 
    }

    pub fn log(&mut self, val:&str){
        self.resources.from_wasm.log(val)
    }

}


// storage buffers for graphics API related resources
#[derive(Clone)]
pub struct CxResources{
    pub from_wasm:FromWasm,
    pub vertex_buffers:usize,
    pub vertex_buffers_free:Vec<usize>,
    pub index_buffers:usize,
    pub index_buffers_free:Vec<usize>,
    pub vaos:usize,
    pub vaos_free:Vec<usize>
}

impl Default for CxResources{
    fn default()->CxResources{
        CxResources{
            from_wasm:FromWasm::zero(),
            vertex_buffers:1,
            vertex_buffers_free:Vec::new(),
            index_buffers:1,
            index_buffers_free:Vec::new(),
            vaos:1,
            vaos_free:Vec::new()
        }
    }
}

impl CxResources{
    fn get_free_vertex_buffer(&mut self)->usize{
        if self.vertex_buffers_free.len() > 0{
            self.vertex_buffers_free.pop().unwrap()
        }
        else{
            self.vertex_buffers += 1;
            self.vertex_buffers
        }
    }
    fn get_free_index_buffer(&mut self)->usize{
        if self.index_buffers_free.len() > 0{
            self.index_buffers_free.pop().unwrap()
        }
        else{
            self.index_buffers += 1;
            self.index_buffers
        }
    }
     fn get_free_vao(&mut self)->usize{
        if self.vaos_free.len() > 0{
            self.vaos_free.pop().unwrap()
        }
        else{
            self.vaos += 1;
            self.vaos
        }
    }
}



#[derive(Clone, Default)]
pub struct DrawListResources{
}

#[derive(Default,Clone)]
pub struct DrawCallResources{
    pub resource_shader_id:usize,
    pub vao_id:usize,
    pub inst_vb_id:usize
}

#[derive(Clone, Default)]
pub struct CxShaders{
    pub compiled_shaders: Vec<CompiledShader>,
    pub shaders: Vec<Shader>,
}

impl DrawCallResources{

    pub fn check_attached_vao(&mut self, csh:&CompiledShader, resources:&mut CxResources){
        if self.resource_shader_id != csh.shader_id{
            self.free(resources); // dont reuse vaos accross shader ids
        }
        // create the VAO
        self.resource_shader_id = csh.shader_id;

        // get a free vao ID
        self.vao_id = resources.get_free_vao();
        self.inst_vb_id = resources.get_free_index_buffer();

        resources.from_wasm.alloc_array_buffer(
            self.inst_vb_id,0,0 as *const f32
        );

        resources.from_wasm.alloc_vao(
            csh.shader_id,
            self.vao_id,
            csh.geom_ib_id,
            csh.geom_vb_id,
            self.inst_vb_id,
        );
    }

    fn free(&mut self, resources:&mut CxResources){
        resources.vaos_free.push(self.vao_id);
        resources.vertex_buffers_free.push(self.inst_vb_id);
        self.vao_id = 0;
        self.inst_vb_id = 0;
    }
}


#[derive(Clone)]
pub struct FromWasm{
    mu32:*mut u32,
    mf32:*mut f32,
    mf64:*mut f64,
    slots:usize,
    used:isize,
    offset:isize
}

impl FromWasm{
    pub fn zero()->FromWasm{
        FromWasm{
            mu32:0 as *mut u32,
            mf32:0 as *mut f32,
            mf64:0 as *mut f64,
            slots:0,
            used:0,
            offset:0
        }
    }
    pub fn new()->FromWasm{
        unsafe{
            let start_bytes = 4096; 
            let buf = alloc::alloc(alloc::Layout::from_size_align(start_bytes as usize, mem::align_of::<u32>()).unwrap()) as *mut u32;
            (buf as *mut u64).write(start_bytes as u64);
            FromWasm{
                mu32:buf as *mut u32,
                mf32:buf as *mut f32,
                mf64:buf as *mut f64,
                slots:start_bytes>>2,
                used:2,
                offset:0
            }
        }
    }

    // fit enough size for RPC structure with exponential alloc strategy
    // returns position to write to
    fn fit(&mut self, slots:usize){
        unsafe{
            if self.used as usize + slots> self.slots{
                let mut new_slots = usize::max(self.used as usize + slots, self.slots * 2);
                if new_slots&1 != 0{ // f64 align
                    new_slots += 1;
                }
                let new_bytes = new_slots<<2;
                let old_bytes = self.slots<<2;
                let new_buf = alloc::alloc(alloc::Layout::from_size_align(new_bytes as usize, mem::align_of::<u64>()).unwrap()) as *mut u32;
                ptr::copy_nonoverlapping(self.mu32, new_buf, self.slots);
                alloc::dealloc(self.mu32 as *mut u8, alloc::Layout::from_size_align(old_bytes as usize, mem::align_of::<u64>()).unwrap());
                self.slots = new_slots;
                (new_buf as *mut u64).write(new_bytes as u64);
                self.mu32 = new_buf;
                self.mf32 = new_buf as *mut f32;
                self.mf64 = new_buf as *mut f64;
            }
            self.offset = self.used;
            self.used += slots as isize;
        }
    }

    fn check(&mut self){
        if self.offset != self.used{
            panic!("Unequal allocation and writes")
        }
    }

    fn mu32(&mut self, v:u32){
        unsafe{
            self.mu32.offset(self.offset).write(v);
            self.offset += 1;
        }
    }

    fn mf32(&mut self, v:f32){
        unsafe{
            self.mf32.offset(self.offset).write(v);
            self.offset += 1;
        }
    }   
 
    fn mf64(&mut self, v:f64){
        unsafe{
            if self.offset&1 != 0{
                self.offset += 1;
            }
            self.mf64.offset(self.offset>>1).write(v);
            self.offset += 2;
        }
    }

    // end the block and return ownership of the pointer
    pub fn end(&mut self){
        self.fit(1);
        self.mu32(0);
    }

    pub fn wasm_ptr(&self)->u32{
        self.mu32 as u32
    }

    fn add_shvarvec(&mut self, shvars:&Vec<ShVar>){
        self.fit(1);
        self.mu32(shvars.len() as u32);
        for shvar in shvars{
            self.add_string(&shvar.ty);
            self.add_string(&shvar.name);
        }
    }

    // log a string
    pub fn log(&mut self, msg:&str){
        self.fit(1);
        self.mu32(1);
        self.add_string(msg);
    }

    pub fn compile_webgl_shader(&mut self, shader_id:usize, ash:&AssembledGLShader){
        self.fit(2);
        self.mu32(2);
        self.mu32(shader_id as u32);
        self.add_string(&ash.fragment);
        self.add_string(&ash.vertex);
        self.fit(2);
        self.mu32(ash.geometry_slots as u32);
        self.mu32(ash.instance_slots as u32);
        self.add_shvarvec(&ash.uniforms_cx);
        self.add_shvarvec(&ash.uniforms_dl);
        self.add_shvarvec(&ash.uniforms_dr);
        self.add_shvarvec(&ash.texture_slots);
    }   

    pub fn alloc_array_buffer(&mut self, buffer_id:usize, len:usize, data:*const f32){
        self.fit(4);
        self.mu32(3);
        self.mu32(buffer_id as u32);
        self.mu32(len as u32);
        self.mu32(data as u32);
    }

    pub fn alloc_index_buffer(&mut self, buffer_id:usize, len:usize, data:*const u32){
        self.fit(4);
        self.mu32(4);
        self.mu32(buffer_id as u32);
        self.mu32(len as u32);
        self.mu32(data as u32);
    }

    pub fn alloc_vao(&mut self, shader_id:usize, vao_id:usize, geom_ib_id:usize, geom_vb_id:usize, inst_vb_id:usize){
        self.fit(6);
        self.mu32(5);
        self.mu32(shader_id as u32);
        self.mu32(vao_id as u32);
        self.mu32(geom_ib_id as u32);
        self.mu32(geom_vb_id as u32);
        self.mu32(inst_vb_id as u32);
    }

    pub fn draw_call(&mut self, shader_id:usize, vao_id:usize, 
        uniforms_cx:&Vec<f32>, uni_cx_update:usize, 
        uniforms_dl:&Vec<f32>, uni_dl_update:usize,
        uniforms_dr:&Vec<f32>, uni_dr_update:usize,
        textures:&Vec<u32>){
        self.fit(10);
        self.mu32(6);
        self.mu32(shader_id as u32);
        self.mu32(vao_id as u32);
        self.mu32(uniforms_cx.as_ptr() as u32);
        self.mu32(uni_cx_update as u32);
        self.mu32(uniforms_dl.as_ptr() as u32);
        self.mu32(uni_dl_update as u32);
        self.mu32(uniforms_dr.as_ptr() as u32);
        self.mu32(uni_dr_update as u32);
        self.mu32(textures.as_ptr() as u32);
    }

    pub fn clear(&mut self, r:f32, g:f32, b:f32, a:f32){
        self.fit(5);
        self.mu32(7);
        self.mf32(r);
        self.mf32(g);
        self.mf32(b);
        self.mf32(a);
    }
   
    pub fn load_deps(&mut self, deps:Vec<String>){
        self.fit(1);
        self.mu32(8);
        self.fit(1);
        self.mu32(deps.len() as u32);
        for dep in deps{
            self.add_string(&dep);
        }
    }

    pub fn alloc_texture(&mut self, texture_id:usize, width:usize, height:usize, data:&Vec<u32>){
        self.fit(5);
        self.mu32(9);
        self.mu32(texture_id as u32);
        self.mu32(width as u32);
        self.mu32(height as u32);
        self.mu32(data.as_ptr() as u32)
    }

    pub fn request_animation_frame(&mut self){
        self.fit(1);
        self.mu32(10);
    }

    pub fn set_document_title(&mut self, title:&str){
        self.fit(1);
        self.mu32(11);
        self.add_string(title);
   }

    fn add_string(&mut self, msg:&str){
        let len = msg.chars().count();
        self.fit(len + 1);
        self.mu32(len as u32);
        for c in msg.chars(){
            self.mu32(c as u32);
        }
        self.check();
    }

}

#[derive(Clone)]
struct ToWasm{
    mu32:*mut u32,
    mf32:*mut f32,
    mf64:*mut f64,
    slots:usize,
    offset:isize
}

impl ToWasm{

    pub fn dealloc(&mut self){
        unsafe{
            alloc::dealloc(self.mu32 as *mut u8, alloc::Layout::from_size_align((self.slots * mem::size_of::<u64>()) as usize, mem::align_of::<u32>()).unwrap());
            self.mu32 = 0 as *mut u32;
            self.mf32 = 0 as *mut f32;
            self.mf64 = 0 as *mut f64;
        }
    }

    pub fn from(buf:u32)->ToWasm{
        unsafe{
            let bytes = (buf as *mut u64).read() as usize;
            ToWasm{
                mu32: buf as *mut u32,
                mf32: buf as *mut f32,
                mf64: buf as *mut f64,
                offset: 2,
                slots: bytes>>2
            }
        }
    }

    fn mu32(&mut self)->u32{
        unsafe{
            let ret = self.mu32.offset(self.offset).read();
            self.offset += 1;
            ret
        }
    }

    fn mf32(&mut self)->f32{
        unsafe{
            let ret = self.mf32.offset(self.offset).read();
            self.offset += 1;
            ret
        }
    }   

    fn mf64(&mut self)->f64{
        unsafe{
            if self.offset&1 != 0{
                self.offset +=1;
            }
            let ret = self.mf64.offset(self.offset>>1).read();
            self.offset += 2;
            ret
        }
    }   

    fn parse_string(&mut self)->String{
        let len = self.mu32();
        let mut out = String::new();
        for _i in 0..len{
            if let Some(c) = std::char::from_u32(self.mu32()) {
                out.push(c);
            }
        }
        out
    }
}


// for use with message passing
#[export_name = "alloc_wasm_buffer"]
pub unsafe extern "C" fn alloc_wasm_buffer(bytes:u32)->u32{
    let buf = std::alloc::alloc(std::alloc::Layout::from_size_align(bytes as usize, mem::align_of::<u64>()).unwrap()) as u32;
    (buf as *mut u64).write(bytes as u64);
    buf as u32
}

// for use with message passing
#[export_name = "realloc_wasm_buffer"]
pub unsafe extern "C" fn realloc_wasm_buffer(in_buf:u32, new_bytes:u32)->u32{
    let old_buf = in_buf as *mut u8;
    let old_bytes = (old_buf as *mut u64).read() as usize;
    let new_buf = alloc::alloc(alloc::Layout::from_size_align(new_bytes as usize, mem::align_of::<u64>()).unwrap()) as *mut u8;
    ptr::copy_nonoverlapping(old_buf, new_buf, old_bytes );
    alloc::dealloc(old_buf as *mut u8, alloc::Layout::from_size_align(old_bytes as usize, mem::align_of::<u64>()).unwrap());
    (new_buf as *mut u64).write(new_bytes as u64);
    new_buf as u32
}

#[export_name = "dealloc_wasm_buffer"]
pub unsafe extern "C" fn dealloc_wasm_buffer(in_buf:u32){
    let buf = in_buf as *mut u8;
    let bytes = buf.read() as usize;
    std::alloc::dealloc(buf as *mut u8, std::alloc::Layout::from_size_align(bytes as usize, mem::align_of::<u64>()).unwrap());
}