use crate::cx::*;
use crate::cx_dx11::*;
use std::ffi;
use winapi::shared:: {dxgiformat};
use winapi::um:: {d3d11, d3dcommon};
use wio::com::ComPtr;

#[derive(Clone)]
pub struct CxPlatformShader {
    pub geom_vbuf: D3d11Buffer,
    pub geom_ibuf: D3d11Buffer,
    pub pixel_shader: ComPtr<d3d11::ID3D11PixelShader>,
    pub vertex_shader: ComPtr<d3d11::ID3D11VertexShader>,
    pub pixel_shader_blob: ComPtr<d3dcommon::ID3DBlob>,
    pub vertex_shader_blob: ComPtr<d3dcommon::ID3DBlob>,
    pub input_layout: ComPtr<d3d11::ID3D11InputLayout>
}

impl Cx {
    pub fn hlsl_compile_all_shaders(&mut self, d3d11_cx: &D3d11Cx) {
        for sh in &mut self.shaders {
            let err = Self::hlsl_compile_shader(sh, d3d11_cx);
            if let Err(err) = err {
                panic!("Got hlsl shader compile error: {}", err.msg);
            }
        };
    }
    
    pub fn hlsl_type(ty: &str) -> String {
        match ty.as_ref() {
            "float" => "float".to_string(),
            "vec2" => "float2".to_string(),
            "vec3" => "float3".to_string(),
            "vec4" => "float4".to_string(),
            "mat2" => "float2x2".to_string(),
            "mat3" => "float3x3".to_string(),
            "mat4" => "float4x4".to_string(),
            "texture2d" => "Texture2D".to_string(),
            ty => ty.to_string()
        }
    }
    
    pub fn hlsl_assemble_struct(lead: &str, name: &str, vars: &Vec<ShVar>, semantic: &str, field: &str, post: &str) -> String {
        let mut out = String::new();
        out.push_str(lead);
        out.push_str(" ");
        out.push_str(name);
        out.push_str(post);
        out.push_str("{\n");
        out.push_str(field);
        for var in vars {
            out.push_str("  ");
            out.push_str(&Self::hlsl_type(&var.ty));
            out.push_str(" ");
            out.push_str(&var.name);
            if semantic.len()>0 {
                out.push_str(": ");
                out.push_str(&format!("{}{}", semantic, var.name.to_uppercase()));
                //out.push_str(&format!("{}", index));
            }
            out.push_str(";\n")
        };
        out.push_str("};\n\n");
        out
    }

    pub fn hlsl_init_struct(vars: &Vec<ShVar>, field: &str) -> String {
        let mut out = String::new();
        out.push_str("{\n");
        out.push_str(field);
        for var in vars {
            out.push_str(match Self::hlsl_type(&var.ty).as_ref() {
                "float" => "0.0",
                "float2" => "float2(0.0,0.0)",
                "float3" => "float3(0.0,0.0,0.0)",
                "float4" => "float4(0.0,0.0,0.0,0.0)",
                _ => "",
            });
            out.push_str(",")
        };
        out.push_str("}");
        out
    }
    
    pub fn hlsl_assemble_texture_slots(textures: &Vec<ShVar>) -> String {
        let mut out = String::new();
        for (i, tex) in textures.iter().enumerate() {
            out.push_str("Texture2D ");
            out.push_str(&tex.name);
            out.push_str(&format!(": register(t{});\n", i));
        };
        out
    }
    
    pub fn hlsl_assemble_shader(sg: &ShaderGen) -> Result<(String, CxShaderMapping), SlErr> {
        
        let mut hlsl_out = String::new();
        
        hlsl_out.push_str("SamplerState DefaultTextureSampler{Filter = MIN_MAG_MIP_LINEAR;AddressU = Wrap;AddressV=Wrap;};\n");
        
        // ok now define samplers from our sh.
        let texture_slots = sg.flat_vars(ShVarStore::Texture);
        let geometries = sg.flat_vars(ShVarStore::Geometry);
        let instances = sg.flat_vars(ShVarStore::Instance);
        let mut varyings = sg.flat_vars(ShVarStore::Varying);
        let locals = sg.flat_vars(ShVarStore::Local);
        let uniforms_cx = sg.flat_vars(ShVarStore::UniformCx);
        let uniforms_vw = sg.flat_vars(ShVarStore::UniformVw);
        let uniforms_dr = sg.flat_vars(ShVarStore::Uniform);
        
        // lets count the slots
        let geometry_slots = sg.compute_slot_total(&geometries);
        let instance_slots = sg.compute_slot_total(&instances);
        //let varying_slots = sh.compute_slot_total(&varyings);
        hlsl_out.push_str(&Self::hlsl_assemble_texture_slots(&texture_slots));
        
        hlsl_out.push_str(&Self::hlsl_assemble_struct("struct", "_Geom", &geometries, "GEOM_", "", ""));
        hlsl_out.push_str(&Self::hlsl_assemble_struct("struct", "_Inst", &instances, "INST_", "", ""));
        hlsl_out.push_str(&Self::hlsl_assemble_struct("cbuffer", "_Uni_Cx", &uniforms_cx, "", "", ": register(b0)"));
        hlsl_out.push_str(&Self::hlsl_assemble_struct("cbuffer", "_Uni_Vw", &uniforms_vw, "", "", ": register(b1)"));
        hlsl_out.push_str(&Self::hlsl_assemble_struct("cbuffer", "_Uni_Dr", &uniforms_dr, "", "", ": register(b2)"));
        hlsl_out.push_str(&Self::hlsl_assemble_struct("struct", "_Loc", &locals, "", "", ""));
        
        // we need to figure out which texture slots exist
        // we need to figure out which texture slots exist
        // mtl_out.push_str(&Self::assemble_constants(&texture_slots));
        
        let mut const_cx = SlCx {
            depth: 0,
            target: SlTarget::Constant,
            defargs_fn: "".to_string(),
            defargs_call: "".to_string(),
            call_prefix: "_".to_string(),
            shader_gen: sg,
            scope: Vec::new(),
            fn_deps: Vec::new(),
            fn_done: Vec::new(),
            auto_vary: Vec::new()
        };
        let consts = sg.flat_consts();
        for cnst in &consts {
            let const_init = assemble_const_init(cnst, &mut const_cx) ?;
            hlsl_out.push_str("#define ");
            hlsl_out.push_str(" ");
            hlsl_out.push_str(&cnst.name);
            hlsl_out.push_str(" (");
            hlsl_out.push_str(&const_init.sl);
            hlsl_out.push_str(")\n");
        }
        
        let mut vtx_cx = SlCx {
            depth: 0,
            target: SlTarget::Vertex,
            defargs_fn: "inout _Loc _loc, inout _Vary _vary, in _Geom _geom, in _Inst _inst".to_string(),
            defargs_call: "_loc, _vary, _geom, _inst".to_string(),
            call_prefix: "_".to_string(),
            shader_gen: sg,
            scope: Vec::new(),
            fn_deps: vec!["vertex".to_string()],
            fn_done: Vec::new(),
            auto_vary: Vec::new()
        };
        
        let vtx_fns = assemble_fn_and_deps(sg, &mut vtx_cx) ?;
        let mut pix_cx = SlCx {
            depth: 0,
            target: SlTarget::Pixel,
            defargs_fn: "inout _Loc _loc, inout _Vary _vary".to_string(),
            defargs_call: "_loc, _vary".to_string(),
            call_prefix: "_".to_string(),
            shader_gen: sg,
            scope: Vec::new(),
            fn_deps: vec!["pixel".to_string()],
            fn_done: vtx_cx.fn_done,
            auto_vary: Vec::new()
        };
        
        let pix_fns = assemble_fn_and_deps(sg, &mut pix_cx) ?;
        
        // lets add the auto_vary ones to the varyings struct
        for auto in &pix_cx.auto_vary {
            varyings.push(auto.clone());
        }
        hlsl_out.push_str(&Self::hlsl_assemble_struct("struct", "_Vary", &varyings, "VARY_", "  float4 hlsl_position : SV_POSITION;\n", ""));
        
        hlsl_out.push_str("//Vertex shader\n");
        hlsl_out.push_str(&vtx_fns);
        hlsl_out.push_str("//Pixel shader\n");
        hlsl_out.push_str(&pix_fns);
        
        // lets define the vertex shader
        hlsl_out.push_str("_Vary _vertex_shader(_Geom _geom, _Inst _inst, uint inst_id: SV_InstanceID){\n");
        hlsl_out.push_str("  _Loc _loc = ");
        hlsl_out.push_str(&Self::hlsl_init_struct(&locals, ""));
        hlsl_out.push_str(";\n");
        hlsl_out.push_str("  _Vary _vary = ");
        hlsl_out.push_str(&Self::hlsl_init_struct(&varyings, "float4(0.0,0.0,0.0,0.0),"));
        hlsl_out.push_str(";\n");
        hlsl_out.push_str("  _vary.hlsl_position = _vertex(");
        hlsl_out.push_str(&vtx_cx.defargs_call);
        hlsl_out.push_str(");\n\n");
        
        for auto in pix_cx.auto_vary {
            if let ShVarStore::Geometry = auto.store {
                hlsl_out.push_str("       _vary.");
                hlsl_out.push_str(&auto.name);
                hlsl_out.push_str(" = _geom.");
                hlsl_out.push_str(&auto.name);
                hlsl_out.push_str(";\n");
            }
            else if let ShVarStore::Instance = auto.store {
                hlsl_out.push_str("       _vary.");
                hlsl_out.push_str(&auto.name);
                hlsl_out.push_str(" = _inst.");
                hlsl_out.push_str(&auto.name);
                hlsl_out.push_str(";\n");
            }
        }
        
        hlsl_out.push_str("       return _vary;\n");
        hlsl_out.push_str("};\n");
        // then the fragment shader
        hlsl_out.push_str("float4 _pixel_shader(_Vary _vary) : SV_TARGET{\n");
        hlsl_out.push_str("  _Loc _loc = ");
        hlsl_out.push_str(&Self::hlsl_init_struct(&locals, ""));
        hlsl_out.push_str(";\n");
        hlsl_out.push_str("  return _pixel(");
        hlsl_out.push_str(&pix_cx.defargs_call);
        hlsl_out.push_str(");\n};\n");
        
        if sg.log != 0 {
            println!("---- HLSL shader -----");
            let lines = hlsl_out.split('\n');
            for (index, line) in lines.enumerate() {
                println!("{} {}", index + 1, line);
            }
        }

        let named_uniform_props = NamedProps::construct(sg, &uniforms_dr, true);
        Ok((hlsl_out, CxShaderMapping {
            zbias_uniform_prop: named_uniform_props.find_zbias_uniform_prop(),
            rect_instance_props: RectInstanceProps::construct(sg, &instances),
            named_instance_props: NamedProps::construct(sg, &instances, false),
            named_uniform_props,
            geometries: geometries,
            instances: instances,
            geometry_slots: geometry_slots,
            instance_slots: instance_slots,
            uniforms_dr: uniforms_dr,
            uniforms_vw: uniforms_vw,
            uniforms_cx: uniforms_cx,
            texture_slots: texture_slots,
        }))
    }
    
    fn slots_to_dxgi_format(slots: usize) -> u32 {
        match slots {
            1 => dxgiformat::DXGI_FORMAT_R32_FLOAT,
            2 => dxgiformat::DXGI_FORMAT_R32G32_FLOAT,
            3 => dxgiformat::DXGI_FORMAT_R32G32B32_FLOAT,
            4 => dxgiformat::DXGI_FORMAT_R32G32B32A32_FLOAT,
            _ => panic!("slots_to_dxgi_format unsupported slotcount {}", slots)
        }
    }
    
    pub fn hlsl_compile_shader(sh: &mut CxShader, d3d11_cx: &D3d11Cx) -> Result<(), SlErr> {
        let (hlsl, mapping) = Self::hlsl_assemble_shader(&sh.shader_gen) ?;
        
        let vs_blob = d3d11_cx.compile_shader("vs", "_vertex_shader".as_bytes(), hlsl.as_bytes()) ?;
        let ps_blob = d3d11_cx.compile_shader("ps", "_pixel_shader".as_bytes(), hlsl.as_bytes()) ?;
        
        let vs = d3d11_cx.create_vertex_shader(&vs_blob) ?;
        let ps = d3d11_cx.create_pixel_shader(&ps_blob) ?;
        
        let mut layout_desc = Vec::new();
        let geom_named = NamedProps::construct(&sh.shader_gen, &mapping.geometries, false);
        let inst_named = NamedProps::construct(&sh.shader_gen, &mapping.instances, false);
        let mut strings = Vec::new();
        
        for geom in &geom_named.props {
            strings.push(ffi::CString::new(format!("GEOM_{}", geom.name.to_uppercase())).unwrap());
            layout_desc.push(d3d11::D3D11_INPUT_ELEMENT_DESC {
                SemanticName: strings.last().unwrap().as_ptr() as *const _,
                SemanticIndex: 0,
                Format: Self::slots_to_dxgi_format(geom.slots),
                InputSlot: 0,
                AlignedByteOffset: (geom.offset * 4) as u32,
                InputSlotClass: d3d11::D3D11_INPUT_PER_VERTEX_DATA,
                InstanceDataStepRate: 0
            })
        }
        
        for inst in &inst_named.props {
            strings.push(ffi::CString::new(format!("INST_{}", inst.name.to_uppercase())).unwrap());
            layout_desc.push(d3d11::D3D11_INPUT_ELEMENT_DESC {
                SemanticName: strings.last().unwrap().as_ptr() as *const _,
                SemanticIndex: 0,
                Format: Self::slots_to_dxgi_format(inst.slots),
                InputSlot: 1,
                AlignedByteOffset: (inst.offset * 4) as u32,
                InputSlotClass: d3d11::D3D11_INPUT_PER_INSTANCE_DATA,
                InstanceDataStepRate: 1
            })
        }
        
        let input_layout = d3d11_cx.create_input_layout(&vs_blob, &layout_desc) ?;

        sh.mapping = mapping;
        sh.platform = Some(CxPlatformShader{
            geom_ibuf: {
                let mut geom_ibuf = D3d11Buffer {..Default::default()};
                geom_ibuf.update_with_u32_index_data(d3d11_cx, &sh.shader_gen.geometry_indices);
                geom_ibuf
            },
            geom_vbuf: {
                let mut geom_vbuf = D3d11Buffer {..Default::default()};
                geom_vbuf.update_with_f32_vertex_data(d3d11_cx, &sh.shader_gen.geometry_vertices);
                geom_vbuf
            },
            vertex_shader: vs,
            pixel_shader: ps,
            vertex_shader_blob: vs_blob,
            pixel_shader_blob: ps_blob,
            input_layout: input_layout,
        });
        
        Ok(())
    }
}


impl<'a> SlCx<'a> {
    pub fn map_call(&self, name: &str, args: &Vec<Sl>) -> MapCallResult {
        match name {
            "sample2d" => { // transform call to
                let base = &args[0];
                let coord = &args[1];
                return MapCallResult::Rewrite(
                    format!("{}.Sample(DefaultTextureSampler,{})", base.sl, coord.sl),
                    "vec4".to_string()
                )
            },
            "color" => {
                let col = color(&args[0].sl);
                return MapCallResult::Rewrite(
                    format!("float4({},{},{},{})", col.r, col.g, col.b, col.a),
                    "vec4".to_string()
                );
            },
            "mix" => {
                return MapCallResult::Rename("lerp".to_string())
            },
            "dfdx" => {
                return MapCallResult::Rename("ddx".to_string())
            },
            "dfdy" => {
                return MapCallResult::Rename("ddy".to_string())
            },
            "atan" => {
                return MapCallResult::Rename("atan2".to_string())
            },
            _ => return MapCallResult::None
        }
    }
    
    pub fn mat_mul(&self, left: &str, right: &str) -> String {
        format!("mul({},{})", left, right)
    }
    
    pub fn map_type(&self, ty: &str) -> String {
        Cx::hlsl_type(ty)
    }
    
    pub fn map_var(&mut self, var: &ShVar) -> String {
        //let mty = Cx::hlsl_type(&var.ty);
        match var.store {
            ShVarStore::Uniform => return var.name.clone(), //format!("_uni_dr.{}", var.name),
            ShVarStore::UniformVw => return var.name.clone(), //format!("_uni_dl.{}", var.name),
            ShVarStore::UniformCx => return var.name.clone(), //format!("_uni_cx.{}", var.name),
            ShVarStore::Instance => {
                if let SlTarget::Pixel = self.target {
                    if self.auto_vary.iter().find( | v | v.name == var.name).is_none() {
                        self.auto_vary.push(var.clone());
                    }
                    return format!("_vary.{}", var.name);
                }
                else {
                    return format!("_inst.{}", var.name);
                }
            },
            ShVarStore::Geometry => {
                if let SlTarget::Pixel = self.target {
                    if self.auto_vary.iter().find( | v | v.name == var.name).is_none() {
                        self.auto_vary.push(var.clone());
                    }
                    return format!("_vary.{}", var.name);
                }
                else {
                    
                    return format!("_geom.{}", var.name);
                }
            },
            ShVarStore::Texture => return var.name.clone(),
            ShVarStore::Local => return format!("_loc.{}", var.name),
            ShVarStore::Varying => return format!("_vary.{}", var.name),
        }
    }
}