use crate::cx::*;

#[derive(Clone)]
pub struct Blit {
    pub shader: Shader,
    pub tx1: f32,
    pub ty1: f32,
    pub tx2: f32,
    pub ty2: f32,
    pub alpha: f32,
    pub do_scroll: bool
}

impl Blit {
    pub fn style(cx: &mut Cx) -> Self {
        Self {
            alpha: 1.0,
            tx1:0.0,
            ty1:0.0,
            tx2:1.0,
            ty2:1.0,
            shader: cx.add_shader(Self::def_blit_shader(), "Blit"),
            do_scroll:false,
        }
    }
    
    pub fn def_blit_shader() -> ShaderGen {
        // lets add the draw shader lib
        let mut sb = ShaderGen::new();
        
        sb.geometry_vertices = vec![0.0, 0.0, 1.0, 0.0, 1.0, 1.0, 0.0, 1.0];
        sb.geometry_indices = vec![0, 1, 2, 2, 3, 0];
        
        sb.compose(shader_ast!({
            
            let geom: vec2<Geometry>;
            let x: float<Instance>;
            let y: float<Instance>;
            let w: float<Instance>;
            let h: float<Instance>;
            let tx1: float<Instance>;
            let ty1: float<Instance>;
            let tx2: float<Instance>;
            let ty2: float<Instance>;
            let tc: vec2<Varying>;
            let view_do_scroll: float<Uniform>;
            let alpha: float<Uniform>;
            let texturez:texture2d<Texture>;
            //let dpi_dilate: float<Uniform>;
            
            fn vertex() -> vec4 {
                // return vec4(geom.x-0.5, geom.y, 0., 1.);
                let shift: vec2 = -view_scroll * view_do_scroll;
                let clipped: vec2 = clamp(
                    geom * vec2(w, h) + vec2(x, y) + shift,
                    view_clip.xy,
                    view_clip.zw
                ); 
                let pos = (clipped - shift - vec2(x, y)) / vec2(w, h);
                tc = mix(vec2(tx1,ty1), vec2(tx2,ty2), pos);
                // only pass the clipped position forward
                return camera_projection * vec4(clipped.x, clipped.y, 0., 1.);
            }
            
            fn pixel() -> vec4 {
                return vec4(sample2d(texturez, tc.xy).rgb, alpha);
            }
            
        }))
    }
    
    
    pub fn begin_blit(&mut self, cx: &mut Cx, texture:&Texture, layout: &Layout) -> InstanceArea {
        let inst = self.draw_blit(cx, texture, Rect::zero());
        let area = inst.clone().into_area();
        cx.begin_turtle(layout, area);
        inst
    }
    
    pub fn end_blit(&mut self, cx: &mut Cx, inst: &InstanceArea) -> Area {
        let area = inst.clone().into_area();
        let rect = cx.end_turtle(area);
        area.set_rect(cx, &rect);
        area
    }
    
    pub fn draw_blit_walk(&mut self, cx: &mut Cx, texture:&Texture, w: Bounds, h: Bounds, margin: Margin) -> InstanceArea {
        let geom = cx.walk_turtle(w, h, margin, None);
        let inst = self.draw_blit_abs(cx, texture, geom);
        cx.align_instance(inst);
        inst
    }
    
    pub fn draw_blit(&mut self, cx: &mut Cx, texture:&Texture, rect: Rect) -> InstanceArea {
        let pos = cx.get_turtle_origin();
        let inst = self.draw_blit_abs(cx, texture, Rect {x: rect.x + pos.x, y: rect.y + pos.y, w: rect.w, h: rect.h});
        cx.align_instance(inst);
        inst
    }
    
    pub fn draw_blit_abs(&mut self, cx: &mut Cx, texture:&Texture, rect: Rect) -> InstanceArea {
        let inst = cx.new_instance_draw_call(&self.shader, 1);
        if inst.need_uniforms_now(cx) {
            inst.push_uniform_float(cx, if self.do_scroll {1.0}else {0.0});
            inst.push_uniform_float(cx, self.alpha);
            inst.push_uniform_texture_2d(cx, texture);
        }
        //println!("{:?} {}", area, cx.current_draw_list_id);
        let data = [
            /*x,y,w,h*/rect.x,
            rect.y,
            rect.w,
            rect.h,
            self.tx1,
            self.ty1,
            self.tx2,
            self.ty2
        ];
        inst.push_slice(cx, &data);
        inst
    }
}
