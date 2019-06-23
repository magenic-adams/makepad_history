/// This is Makepad, a work-in-progress livecoding IDE for 2D Design.
// This application is nearly 100% Wasm running on webGL. NO HTML AT ALL.
// The vision is to build a livecoding / design hybrid program,
// here procedural design and code are fused in one environment.
// If you have missed 'learnable programming' please check this out:
// http://worrydream.com/LearnableProgramming/
// Makepad aims to fulfill (some) of these ideas using a completely
// from-scratch renderstack built on the GPU and Rust/wasm.
// It will be like an IDE meets a vector designtool, and had offspring.
// Direct manipulation of the vectors modifies the code, the code modifies the vectors.
// And the result can be lasercut, embroidered or drawn using a plotter.
// This means our first goal is 2D path CNC with booleans (hence the CAD),
// and not dropshadowy-gradients.

// Find the repo and more explanation at github.com/makepad/makepad.
// We are developing the UI kit and code-editor as MIT, but the full stack
// will be a cloud/native app product in a few months.

// However to get to this amazing mixed-mode code editing-designtool,
// we first have to build an actually nice code editor (what you are looking at)
// And a vector stack with booleans (in progress)
// Up next will be full multiplatform support and more visual UI.
// All of the code is written in Rust, and it compiles to native and Wasm
// Its built on a novel immediate-mode UI architecture
// The styling is done with shaders written in Rust, transpiled to metal/glsl

// for now enjoy how smooth a full GPU editor can scroll (try select scrolling the edge)
// Also the tree fold-animates and the docking panel system works.
// Multi cursor/grid cursor also works with ctrl+click / ctrl+shift+click
// press alt or escape for animated codefolding outline view!

use render::*;
use widget::*;
pub mod cap_winmf;
pub use crate::capture_winmf::*;
use core::arch::x86_64::*;

#[derive(Default)]
struct Mandelbrot {
    texture: Texture,
    num_threads: usize,
    width: usize,
    zoom: f64,
    height: usize, 
    frame_signal: Signal,
}

impl Mandelbrot {
    fn init(&mut self, cx: &mut Cx) {
        // lets start a mandelbrot thread that produces frames
        self.frame_signal = cx.new_signal();
        self.width = 3840; 
        self.height = 2160;
        self.zoom = 1.0;
        self.num_threads = 30;
        self.texture.set_desc(cx, Some(TextureDesc {
            format: TextureFormat::MappedRGf32,
            width: Some(self.width),
            height: Some(self.height),
            multisample: None
        }));

        #[inline]
        unsafe fn calc_mandel_avx2(c_x: __m256d, c_y: __m256d, max_iter: u32) -> (__m256d, __m256d) {
            let mut x = c_x;
            let mut y = c_y;
            let mut count = _mm256_set1_pd(0.0);
            let mut merge_sum = _mm256_set1_pd(0.0);
            let add = _mm256_set1_pd(1.0);
            let max_dist = _mm256_set1_pd(4.0);
            
            for _ in 0..max_iter as usize {
                let xy = _mm256_mul_pd(x, y);
                let xx = _mm256_mul_pd(x, x);
                let yy = _mm256_mul_pd(y, y);
                let sum = _mm256_add_pd(xx, yy);
                let mask = _mm256_cmp_pd(sum, max_dist, _CMP_LT_OS);
                let mask_u32 = _mm256_movemask_pd(mask);
                if mask_u32 == 0 { // is a i8
                    return (count, _mm256_sqrt_pd(merge_sum));
                }
                merge_sum = _mm256_or_pd(_mm256_and_pd(sum, mask), _mm256_andnot_pd(mask, merge_sum));
                count = _mm256_add_pd(count, _mm256_and_pd(add, mask));
                x = _mm256_add_pd(_mm256_sub_pd(xx, yy), c_x);
                y = _mm256_add_pd(_mm256_add_pd(xy, xy), c_y);
            }
            return (count, add);
        }
        /*
        fn calc_mandel(x: f64, y: f64) -> u32 {
            let mut rr = x;
            let mut ri = y;
            let mut n = 0;
            while n < 80 {
                let mut tr = rr * rr - ri * ri + x;
                let mut ti = 2.0 * rr * ri + y;
                rr = tr;
                ri = ti;
                n = n + 1;
                if rr * ri > 5.0 {
                    return n;
                }
            }
            return n;
        }*/

        // lets spawn fractal.height over 32 threads
        let num_threads = self.num_threads;
        let width = self.width;
        let height = self.height;
        let fwidth = self.width as f64;
        let fheight = self.height as f64;
        let chunk_height = height / num_threads;
        
        // stuff that goes into the threads
        let mut thread_pool = scoped_threadpool::Pool::new(self.num_threads as u32);
        let frame_signal = self.frame_signal.clone();
        let mut cxthread = cx.new_cxthread();
        let texture = self.texture.clone();
        
        std::thread::spawn(move || {
            let mut zoom = 1.0;
            let center_x = -1.5;
            let center_y = 0.0;
            loop {
                zoom = zoom / 1.003;
                if zoom < 0.000000000002 {
                    zoom = 1.0;
                }
                //let time1 = Cx::profile_time_ns();

                thread_pool.scoped( | scope | {
                    if let Some(mapped_texture) = cxthread.lock_mapped_texture_f32(&texture){
                        let mut iter = mapped_texture.chunks_mut((chunk_height * width * 2) as usize);
                        for i in 0..num_threads {
                            let thread_num = i;
                            let slice = iter.next().unwrap();
                            //println!("{}", chunk_height);
                            scope.execute(move || {
                                let it_v = [0f64, 0f64, 0f64, 0f64];
                                let su_v = [0f64, 0f64, 0f64, 0f64];
                                let start = thread_num * chunk_height as usize;
                                for y in (start..(start + chunk_height)).step_by(2) {
                                    for x in (0..width).step_by(2) {
                                        unsafe {
                                            // TODO simd these things too
                                            let c_re = _mm256_set_pd(
                                                (x as f64 - fwidth * 0.5) * 4.0 / fwidth * zoom + center_x,
                                                ((x + 1) as f64 - fwidth * 0.5) * 4.0 / fwidth * zoom + center_x,
                                                (x as f64 - fwidth * 0.5) * 4.0 / fwidth * zoom + center_x,
                                                ((x + 1) as f64 - fwidth * 0.5) * 4.0 / fwidth * zoom + center_x
                                            );
                                            let c_im = _mm256_set_pd(
                                                (y as f64 - fheight * 0.5) * 3.0 / fheight * zoom + center_y,
                                                (y as f64 - fheight * 0.5) * 3.0 / fheight * zoom + center_y,
                                                ((y + 1) as f64 - fheight * 0.5) * 3.0 / fheight * zoom + center_y,
                                                ((y + 1) as f64 - fheight * 0.5) * 3.0 / fheight * zoom + center_y
                                            );
                                            let (it256, sum256) = calc_mandel_avx2(c_re, c_im, 130);
                                            _mm256_store_pd(it_v.as_ptr(), it256);
                                            _mm256_store_pd(su_v.as_ptr(), sum256);
                                            
                                            slice[(x * 2 + (y - start) * width * 2) as usize] = it_v[3] as f32;
                                            slice[(x * 2 + 2 + (y - start) * width * 2) as usize] = it_v[2] as f32;
                                            slice[(x * 2 + (1 + y - start) * width * 2) as usize] = it_v[1] as f32;
                                            slice[(x * 2 + 2 + (1 + y - start) * width * 2) as usize] = it_v[0] as f32;
                                            slice[1 + (x * 2 + (y - start) * width * 2) as usize] = su_v[3] as f32;
                                            slice[1 + (x * 2 + 2 + (y - start) * width * 2) as usize] = su_v[2] as f32;
                                            slice[1 + (x * 2 + (1 + y - start) * width * 2) as usize] = su_v[1] as f32;
                                            slice[1 + (x * 2 + 2 + (1 + y - start) * width * 2) as usize] = su_v[0] as f32;
                                        }
                                    }
                                }
                            })
                        }
                    }
                    else{ // wait a bit
                        std::thread::sleep(std::time::Duration::from_millis(1));
                        return
                    }
                });
                cxthread.unlock_mapped_texture(&texture);
                //println!("{}", (Cx::profile_time_ns()-time1) as f64/1000.);
                Cx::send_signal(frame_signal, 0);
            }
        });
    }
    
    fn handle_signal(&mut self, _cx: &mut Cx, event: &Event) -> bool {
        if let Event::Signal(se) = event {
            if self.frame_signal.is_signal(se) { // we haz new texture
                return true
            }
        }
        false
    }
}

struct App {
    view: View<ScrollBar>,
    window: DesktopWindow,
    blit: Blit,
    mandel_blit: Blit,
    frame: f32,
    capture: Capture,
    mandel: Mandelbrot,
}

main_app!(App, "Example");

impl Style for App {
    fn style(cx: &mut Cx) -> Self {
        set_dark_style(cx);
        Self {
            mandel: Mandelbrot::default(),
            capture: Capture::default(),
            window: DesktopWindow::style(cx),
            view: View {
                is_overlay: true,
                scroll_h: Some(ScrollBar::style(cx)),
                scroll_v: Some(ScrollBar::style(cx)),
                ..Style::style(cx)
            },
            blit: Blit::style(cx),
            mandel_blit: Blit {
                shader: cx.add_shader(App::def_blit_shader(), "App.blit"),
                ..Blit::style(cx)
            },
            frame: 0.
        }
    }
}

impl App {
    fn def_blit_shader() -> ShaderGen {
        Blit::def_blit_shader().compose(shader_ast!({
            let texcam: texture2d<Texture>;
            let time: float<Uniform>;
            
            
            fn kaleido(uv:vec2, angle:float, base:float, spin:float)->vec2{
                let a = atan(uv.y, uv.x);
                if a<0.{a = PI + a}
                let d = length(uv);
                a = abs(fmod (a + spin, angle * 2.0) - angle);
                return vec2(abs(sin(a + base)) * d, abs(cos(a + base)) * d);
            }
            
            fn pixel() -> vec4 {
                let fr1 = sample2d(texturez, geom.xy).rg;//kaleido(geom.xy-vec2(0.5,0.5), 3.14/8., time, 2.*time)).rg;
                let fr2 = sample2d(texturez, vec2(1.0 - geom.x, geom.y)).rg;
                let cam = sample2d(texcam, geom.xy);//kaleido(geom.xy-vec2(0.5,0.5), 3.14/8., time, 2.*time));
                let fract = df_iq_pal4(time*3. + fr1.x * 0.03 + fr2.x * 0.03 - 0.1 * log(fr1.y * fr2.y));
                return vec4(cam.xyz*fract.xyz, alpha);
            }
        }))
    }
    
    fn handle_app(&mut self, cx: &mut Cx, event: &mut Event) {
        
        if let Event::Construct = event {
            self.capture.init(cx, 0, CaptureFormat::NV12, 3840, 2160, 30.0);
            self.mandel.init(cx);
        }
        
        self.window.handle_desktop_window(cx, event);
        
        self.view.handle_scroll_bars(cx, event);
        
        if let CaptureEvent::NewFrame = self.capture.handle_signal(cx, event) {
            self.view.redraw_view_area(cx);
        }
        
        if self.mandel.handle_signal(cx, event) {
            self.view.redraw_view_area(cx);
        }
    }
    
    fn draw_app(&mut self, cx: &mut Cx) {
        
        let _ = self.window.begin_desktop_window(cx);
        
        let _ = self.view.begin_view(cx, Layout {
            abs_origin: Some(Vec2::zero()),
            padding: Padding {l: 0., t: 0., r: 0., b: 0.},
            ..Default::default()
        });
        
        self.view.redraw_view_area(cx);
        let inst = self.mandel_blit.draw_blit_walk(cx, &self.mandel.texture, Bounds::Fill, Bounds::Fill, Margin::zero());
        inst.push_uniform_texture_2d(cx, &self.capture.texture);
        inst.push_uniform_float(cx, self.frame);
        self.frame += 0.001;
        cx.reset_turtle_walk();
        
        self.view.redraw_view_area(cx);
        self.view.end_view(cx);
        
        self.window.end_desktop_window(cx);
    }
}