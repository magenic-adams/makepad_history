use crate::cx::*;
use std::f64::consts::PI;

#[derive(Clone,Debug)]
pub struct AnimArea{
    pub area:Area,
    pub start_time:f64,
    pub total_time:f64
}

#[derive(Clone,Debug)]
pub struct Anim{
    pub mode:Play,
    pub tracks:Vec<Track>
}

#[derive(Clone)]
pub struct Animator{
    pub default:Anim,
    current:Option<Anim>,
    next:Option<Anim>,
    pub area:Area,
    last_float:Vec<(String, f32)>,
    last_vec2:Vec<(String, Vec2)>,
    last_vec3:Vec<(String, Vec3)>,
    last_vec4:Vec<(String, Vec4)>,
    last_color:Vec<(String, Color)>,
}

impl Animator{

    pub fn new(default:Anim)->Animator{
        Animator{
            default:default,
            current:None,
            next:None,
            area:Area::Empty,
            last_float:Vec::new(),
            last_vec2:Vec::new(),
            last_vec3:Vec::new(),
            last_vec4:Vec::new(),
            last_color:Vec::new(),
        }
    }

    pub fn set_anim_as_last_values(&mut self, anim:&Anim){
        for track in &anim.tracks{
            // we dont have a last float, find it in the tracks
            let ident = track.ident();
            match track{
                Track::Color(ft)=>{
                    let val = if ft.track.len()>0{ft.track.last().unwrap().1}else{Color::zero()};
                    if let Some((_name, v)) = self.last_color.iter_mut().find(|(name,_v)| name == ident){
                        *v = val;
                    }
                    else{ 
                        self.last_color.push((ident.clone(), val));
                    }
                }
                Track::Vec4(ft)=>{
                    let val = if ft.track.len()>0{ft.track.last().unwrap().1}else{Vec4::zero()};
                    if let Some((_name, v)) = self.last_vec4.iter_mut().find(|(name,_v)| name == ident){
                        *v = val;
                    }
                    else{ 
                        self.last_vec4.push((ident.clone(), val));
                    }
                },
                Track::Vec3(ft)=>{
                    let val = if ft.track.len()>0{ft.track.last().unwrap().1}else{Vec3::zero()};
                    if let Some((_name, v)) = self.last_vec3.iter_mut().find(|(name,_v)| name == ident){
                        *v = val;
                    }
                    else{ 
                        self.last_vec3.push((ident.clone(), val));
                    }
                },
                Track::Vec2(ft)=>{
                    let val = if ft.track.len()>0{ft.track.last().unwrap().1}else{Vec2::zero()};
                    if let Some((_name, v)) = self.last_vec2.iter_mut().find(|(name,_v)| name == ident){
                        *v = val;
                    }
                    else{ 
                        self.last_vec2.push((ident.clone(), val));
                    }
                },
                Track::Float(ft)=>{
                    let val = if ft.track.len()>0{ft.track.last().unwrap().1}else{0.};
                    if let Some((_name, v)) = self.last_float.iter_mut().find(|(name,_v)| name == ident){
                        *v = val;
                    }
                    else{ 
                        self.last_float.push((ident.clone(), val));
                    }
                }                
            }
        }
    }

    pub fn term_anim_playing(&mut self)->bool{
        if let Some(current) = &self.current{
            return current.mode.term();
        }
        return false
    }

    pub fn play_anim(&mut self, cx:&mut Cx, anim:Anim){
        // if our area is invalid, we should just set our default value 
        if let Some(current) = &self.current{
            if current.mode.term(){ // can't override a term anim
                return
            }    
        }

        if !self.area.is_valid(cx){
            self.set_anim_as_last_values(&anim);
            self.current = Some(anim);
            return
        }
        // alright first we find area, it already exists
        if let Some(anim_area) = cx.playing_anim_areas.iter_mut().find(|v| v.area == self.area){
            //do we cut the animation in right now?
            if anim.mode.cut(){
                self.current = Some(anim);
                anim_area.start_time = std::f64::NAN;
                self.next = None;
                anim_area.total_time = self.current.as_ref().unwrap().mode.total_time();
            }
            else{ // queue it
                self.next = Some(anim);
                // lets ask an animation anim how long it is
                anim_area.total_time = self.current.as_ref().unwrap().mode.total_time() + self.next.as_ref().unwrap().mode.total_time()
            }
        }
        else if self.area != Area::Empty{ // its new
            self.current = Some(anim);
            self.next = None;
            cx.playing_anim_areas.push(AnimArea{
                area:self.area.clone(),
                start_time:std::f64::NAN,
                total_time:self.current.as_ref().unwrap().mode.total_time()
            })
        }
    }

    pub fn update_area_refs(&mut self, cx:&mut Cx, area:Area){
        if self.area != Area::Empty{
            cx.update_area_refs(self.area, area.clone());
        }
        self.area = area.clone();
    }

    pub fn fetch_calc_track(&mut self, cx:&mut Cx, ident:&str, time:f64)->Option<(f64, usize)>{
        // alright first we find area in running animations
        let anim_index_opt = cx.playing_anim_areas.iter().position(|v| v.area == self.area);
        if anim_index_opt.is_none(){
            return None
        }
        let anim_index = anim_index_opt.unwrap();

        // initialize start time
        if cx.playing_anim_areas[anim_index].start_time.is_nan(){
            cx.playing_anim_areas[anim_index].start_time = time;
        }
        let mut start_time = cx.playing_anim_areas[anim_index].start_time;
        
        // fetch current anim
        if self.current.is_none(){  // remove anim
            cx.playing_anim_areas.remove(anim_index);
            return None
        }
        
        let current_total_time = self.current.as_ref().unwrap().mode.total_time();

        let current_time;
         
        // process queueing
        if time - start_time >=  current_total_time && !self.next.is_none(){ // we are still here, check if we have a next anim
            self.current = self.next.clone();
            self.next = None;
            // update animation slot
            start_time += current_total_time;
            if let Some(anim) = cx.playing_anim_areas.iter_mut().find(|v| v.area == self.area){
                anim.start_time = start_time;
                anim.total_time -= current_total_time;
            }
            current_time = self.current.as_ref().unwrap().mode.compute_time(time - start_time);
        }
        else{
            current_time = self.current.as_ref().unwrap().mode.compute_time(time - start_time);
        }

        // find our track
        for (track_index, track) in &mut self.current.as_ref().unwrap().tracks.iter().enumerate(){
            if track.ident() == ident{
                return Some((current_time, track_index));
            }
        }
        None
    } 

    pub fn calc_float(&mut self, cx:&mut Cx, ident:&str, time:f64)->f32{
        let last = self.last_float(ident);
        let mut ret = last;
        if let Some((time, track_index)) = self.fetch_calc_track(cx, ident, time){
            if let Track::Float(ft) = &mut self.current.as_mut().unwrap().tracks[track_index]{
                ret = Track::compute_track_value::<f32>(time, &ft.track, &mut ft.cut_init, last, &ft.ease);
            }
        }
        self.set_last_float(ident, ret);
        return ret
    } 

    pub fn last_float(&self, ident:&str)->f32{
        if let Some((_name, v)) = self.last_float.iter().find(|(name,_v)| name == ident){
            return *v;
        }
        // we dont have a last float, find it in the tracks
        if let Some(track) = self.default.tracks.iter().find(|tr| tr.ident() == ident){
            if let Track::Float(ft) = track{
                if ft.track.len()>0{ // grab the last key in the track
                    return ft.track.last().unwrap().1
                }
            }
        }
        return 0.0
    }

    pub fn set_last_float(&mut self, ident:&str, value:f32){
        if let Some(last) = self.last_float.iter_mut().find(|(name,_v)| name == ident){
            last.1 = value;
        }
        else{
            self.last_float.push((ident.to_string(), value))
        }
    }

    pub fn calc_vec2(&mut self, cx:&mut Cx, ident:&str, time:f64)->Vec2{
        let last = self.last_vec2(ident);
        let mut ret = last;
        if let Some((time, track_index)) = self.fetch_calc_track(cx, ident, time){
            if let Track::Vec2(ft) = &mut self.current.as_mut().unwrap().tracks[track_index]{
                ret =  Track::compute_track_value::<Vec2>(time, &ft.track, &mut ft.cut_init, last, &ft.ease);
            }
        }
        self.set_last_vec2(ident, ret);
        return ret
    }

    pub fn last_vec2(&self, ident:&str)->Vec2{
        if let Some((_name, v)) = self.last_vec2.iter().find(|(name,_v)| name == ident){
            return *v;
        }
        // we dont have a last float, find it in the tracks
        if let Some(track) = self.default.tracks.iter().find(|tr| tr.ident() == ident){
            if let Track::Vec2(ft) = track{
                if ft.track.len()>0{ // grab the last key in the track
                    return ft.track.last().unwrap().1
                }
            }
        }
        return Vec2::zero()
    }

    pub fn set_last_vec2(&mut self, ident:&str, value:Vec2){
        if let Some(last) = self.last_vec2.iter_mut().find(|(name,_v)| name == ident){
            last.1 = value;
        }
        else{
            self.last_vec2.push((ident.to_string(), value))
        }
    }

    pub fn calc_vec3(&mut self, cx:&mut Cx, ident:&str, time:f64)->Vec3{
        let last = self.last_vec3(ident);
        let mut ret = last;
        if let Some((time, track_index)) = self.fetch_calc_track(cx, ident, time){
            if let Track::Vec3(ft) = &mut self.current.as_mut().unwrap().tracks[track_index]{
                ret =  Track::compute_track_value::<Vec3>(time, &ft.track, &mut ft.cut_init, last, &ft.ease);
            }
        }
        self.set_last_vec3(ident, ret);
        return ret
    }

    pub fn last_vec3(&self, ident:&str)->Vec3{
        if let Some((_name, v)) = self.last_vec3.iter().find(|(name,_v)| name == ident){
            return *v;
        }
        // we dont have a last float, find it in the tracks
        if let Some(track) = self.default.tracks.iter().find(|tr| tr.ident() == ident){
            if let Track::Vec3(ft) = track{
                if ft.track.len()>0{ // grab the last key in the track
                    return ft.track.last().unwrap().1
                }
            }
        }
        return Vec3::zero()
    }

    pub fn set_last_vec3(&mut self, ident:&str, value:Vec3){
        if let Some(last) = self.last_vec3.iter_mut().find(|(name,_v)| name == ident){
            last.1 = value;
        }
        else{
            self.last_vec3.push((ident.to_string(), value))
        }
    }

    pub fn calc_vec4(&mut self, cx:&mut Cx, ident:&str, time:f64)->Vec4{
        let last = self.last_vec4(ident);
        let mut ret = last;
        if let Some((time, track_index)) = self.fetch_calc_track(cx, ident, time){
            if let Track::Vec4(ft) = &mut self.current.as_mut().unwrap().tracks[track_index]{
                ret =  Track::compute_track_value::<Vec4>(time, &ft.track, &mut ft.cut_init, last, &ft.ease);
            }
        }
        self.set_last_vec4(ident, ret);
        return ret
    }

    pub fn last_vec4(&self, ident:&str)->Vec4{
        if let Some((_name, v)) = self.last_vec4.iter().find(|(name,_v)| name == ident){
            return *v;
        }
        // we dont have a last float, find it in the tracks
        if let Some(track) = self.default.tracks.iter().find(|tr| tr.ident() == ident){
            if let Track::Vec4(ft) = track{
                if ft.track.len()>0{ // grab the last key in the track
                    return ft.track.last().unwrap().1
                }
            }
        }
        return Vec4::zero()
    }

    pub fn set_last_vec4(&mut self, ident:&str, value:Vec4){
        if let Some(last) = self.last_vec4.iter_mut().find(|(name,_v)| name == ident){
            last.1 = value;
        }
        else{
            self.last_vec4.push((ident.to_string(), value))
        }
    }


    pub fn last_color(&self, ident:&str)->Color{
        if let Some((_name, v)) = self.last_color.iter().find(|(name,_v)| name == ident){
            return *v;
        }
        // we dont have a last float, find it in the tracks
        if let Some(track) = self.default.tracks.iter().find(|tr| tr.ident() == ident){
            if let Track::Color(ft) = track{
                if ft.track.len()>0{ // grab the last key in the track
                    return ft.track.last().unwrap().1
                }
            }
        }
        return Color::zero()
    }

    pub fn set_last_color(&mut self, ident:&str, value:Color){
        if let Some(last) = self.last_color.iter_mut().find(|(name,_v)| name == ident){
            last.1 = value;
        }
        else{
            self.last_color.push((ident.to_string(), value))
        }
    }

    pub fn calc_write(&mut self, cx:&mut Cx, ident:&str, time:f64, area:Area){
        if let Some(dot) = ident.find('.'){
            let field = ident.get((dot+1)..ident.len()).unwrap();

            if let Some((time, track_index)) = self.fetch_calc_track(cx, ident, time){
                let track_type = match &mut self.current.as_mut().unwrap().tracks[track_index]{
                    Track::Color(_)=>5,
                    Track::Vec4(_)=>4,
                    Track::Vec3(_)=>3,
                    Track::Vec2(_)=>2,
                    Track::Float(_)=>1
                };
                match track_type {
                    5=>{
                        let init = self.last_color(ident);
                        let ret = if let Track::Color(ft) = &mut self.current.as_mut().unwrap().tracks[track_index]{
                            Track::compute_track_value::<Color>(time, &ft.track, &mut ft.cut_init, init, &ft.ease)
                        }
                        else{
                            Color::zero()
                        };
                        self.set_last_color(ident, ret);
                        area.write_color(cx, field, ret);
                    },
                    4=>{
                        let init = self.last_vec4(ident);
                        let ret = if let Track::Vec4(ft) = &mut self.current.as_mut().unwrap().tracks[track_index]{
                            Track::compute_track_value::<Vec4>(time, &ft.track, &mut ft.cut_init, init, &ft.ease)
                        }
                        else{
                            Vec4::zero()
                        };
                        self.set_last_vec4(ident, ret);
                        area.write_vec4(cx, field, ret);
                    },
                    3=>{
                        let init = self.last_vec3(ident);
                        let ret = if let Track::Vec3(ft) = &mut self.current.as_mut().unwrap().tracks[track_index]{
                            Track::compute_track_value::<Vec3>(time, &ft.track, &mut ft.cut_init, init, &ft.ease)
                        }
                        else{
                            Vec3::zero()
                        };
                        self.set_last_vec3(ident, ret);
                        area.write_vec3(cx, field, ret);
                    },
                    2=>{
                        let init = self.last_vec2(ident);
                        let ret = if let Track::Vec2(ft) = &mut self.current.as_mut().unwrap().tracks[track_index]{
                            Track::compute_track_value::<Vec2>(time, &ft.track, &mut ft.cut_init, init, &ft.ease)
                        }
                        else{
                            Vec2::zero()
                        };
                        self.set_last_vec2(ident, ret);
                        area.write_vec2(cx, field, ret);
                    },
                    1=>{
                        let init = self.last_float(ident);
                        let ret = if let Track::Float(ft) = &mut self.current.as_mut().unwrap().tracks[track_index]{
                            Track::compute_track_value::<f32>(time, &ft.track, &mut ft.cut_init, init, &ft.ease)
                        }
                        else{
                            0.0
                        };
                        self.set_last_float(ident, ret);
                        area.write_float(cx, field, ret);
                    },
                    _=>()
                }
            }
        }
    }

/*
    pub fn last_push(&self, cx: &mut Cx, area_name:&str, area:Area){
        if let Some(dot) = area_name.find('.'){
            let field = area_name.get((dot+1)..area_name.len()).unwrap();

            let anim = if self.current.is_none(){
                &self.default
            }
            else{
                self.current.as_ref().unwrap()
            };
            for track in &anim.tracks{
                if track.ident() == area_name{
                    match track{
                        Track::Vec4(_)=>{
                            let v4 = self.last_vec4(area_name);
                            area.push_vec4(cx, field, v4);
                        },
                        Track::Vec3(_)=>{
                            let v3 = self.last_vec3(area_name);
                            area.push_vec3(cx, field, v3);
                        },
                        Track::Vec2(_)=>{
                            let v2 = self.last_vec2(area_name);
                            area.push_vec2(cx, field, v2);
                        },
                        Track::Float(_)=>{
                            let fl =  self.last_float(area_name);
                            area.push_float(cx, field, fl);
                        }
                    }
                    return
                }
            }

        }
    }*/

}

#[derive(Clone,Debug)]
pub enum Ease{
    Lin,
    InQuad,
    OutQuad,
    InOutQuad,
    InCubic,
    OutCubic,
    InOutCubic,
    InQuart,
    OutQuart,
    InOutQuart,
    InQuint,
    OutQuint,
    InOutQuint,
    InSine,
    OutSine,
    InOutSine,
    InExp,
    OutExp,
    InOutExp,
    InCirc,
    OutCirc,
    InOutCirc,
    InElastic,
    OutElastic,
    InOutElastic,
    InBack,
    OutBack,
    InOutBack,
    InBounce,
    OutBounce,
    InOutBounce,
    Pow{begin:f64, end:f64},
    Bezier{cp0:f64, cp1:f64, cp2:f64, cp3:f64}
    /*
    Bounce{dampen:f64},
    Elastic{duration:f64, frequency:f64, decay:f64, ease:f64}, 
    */
}


impl Ease{
    pub fn map(&self, t:f64)->f64{
        match self{
            Ease::Lin=>{
                return t.max(0.0).min(1.0);
            },
            Ease::Pow{begin, end}=>{
                if t < 0.{
                    return 0.;
                }
                if t > 1. {
                    return 1.;
                }
                let a = -1. / (begin * begin).max(1.0);
                let b = 1. + 1. / (end * end).max(1.0);
                let t2 = (((a - 1.) * -b) / (a * (1. - b))).powf(t);
                return (-a * b + b * a * t2) / (a * t2 - b);
            },

            Ease::InQuad=>{
                return t*t;
            },
            Ease::OutQuad=>{
                return t * (2.0 - t);
            },
            Ease::InOutQuad=>{
                let t = t * 2.0;
                if t < 1.{
                    return 0.5 * t * t;
                }
                else{
                    let t = t - 1.;
                    return -0.5 * (t*(t-2.) - 1.);
                }
            },
            Ease::InCubic=>{
                return t*t*t;
            },
            Ease::OutCubic=>{
                let t2 = t - 1.0;
                return t2*t2*t2 + 1.0;
            },
            Ease::InOutCubic=>{
                let t = t * 2.0;
                if t < 1.{
                    return 0.5 * t * t * t;
                }
                else{
                    let t = t - 2.;
                    return 1. / 2.*(t * t * t + 2.);
                }
            },
            Ease::InQuart=>{
                return t * t * t * t
            },
            Ease::OutQuart=>{
                let t = t - 1.;
                return - (t * t * t * t - 1.);
            },
            Ease::InOutQuart=>{
                let t = t * 2.0;
                if t < 1.{
                    return 0.5 * t * t * t * t;
                }
                else{
                    let t = t - 2.;
                    return -0.5 * (t * t * t * t - 2.);
                }
            },
            Ease::InQuint=>{
               return  t * t * t * t * t;
            },
            Ease::OutQuint=>{
                let t = t - 1.;
                return t * t * t * t * t + 1.;
            },
            Ease::InOutQuint=>{
                let t = t * 2.0;
                if t < 1.{
                    return 0.5 * t * t * t * t * t;
                }
                else{
                    let t = t - 2.;
                    return 0.5 * (t * t * t * t * t + 2.);
                }
            },
            Ease::InSine=>{
                return -(t * PI*0.5).cos() + 1.;
            },
            Ease::OutSine=>{
                return (t * PI*0.5).sin();
            },
            Ease::InOutSine=>{
                return -0.5 * ( (t * PI).cos() - 1.);
            },
            Ease::InExp=>{
                if t < 0.001{
                    return 0.;
                }
                else{
                    return 2.0f64.powf(10. * (t - 1.));
                }
            },
            Ease::OutExp=>{
                if t > 0.999{
                    return 1.;
                }
                else{
                    return -(2.0f64.powf(-10. * t)) + 1.;
                }
            },
            Ease::InOutExp=>{
                if t<0.001{
                    return 0.;
                }
                if t>0.999{
                    return 1.;
                }
                let t = t * 2.0;
                if t < 1.{
                    return 0.5 * 2.0f64.powf( 10. * (t - 1.));
                }
                else{
                    let t = t - 1.;
                    return 0.5 * (-(2.0f64.powf(-10.*t)) + 2.);
                }
            },
            Ease::InCirc=>{
                return -((1. - t * t).sqrt() - 1.);
            },
            Ease::OutCirc=>{
                let t = t - 1.;
                return (1. - t * t).sqrt();
            },
            Ease::InOutCirc=>{
                let t = t * 2.;
                if t < 1.{
                    return - 0.5 * ((1. - t*t).sqrt() - 1.);
                }
                else{
                    let t = t - 2.;
                    return 0.5 * ((1. - t*t).sqrt() + 1.);
                }
            },
            Ease::InElastic=>{
                let p = 0.3;
                let s = p/4.0; // c = 1.0, b = 0.0, d = 1.0
                if t < 0.001{
                    return 0.;
                }
                if t > 0.999{
                    return 1.;
                }
                let t = t - 1.0;
                return -(2.0f64.powf(10.0*t) * ( (t-s)*(2.0*PI)/p ).sin());
            },
            Ease::OutElastic=>{
                let p = 0.3;
                let s = p/4.0; // c = 1.0, b = 0.0, d = 1.0
                
                if t < 0.001{
                    return 0.;
                }
                if t > 0.999{
                    return 1.;
                }
                return 2.0f64.powf(-10.0*t) * ( (t-s)*(2.0*PI)/p ).sin() + 1.0;
            },
            Ease::InOutElastic=>{
                let p = 0.3;
                let s = p/4.0; // c = 1.0, b = 0.0, d = 1.0 
                if t < 0.001{
                    return 0.;
                }
                if t > 0.999{
                    return 1.;
                }
                let t = t * 2.0;
                if t < 1.{
                    let t = t - 1.0;
                    return -0.5 * (2.0f64.powf(10.0*t) * ( (t-s)*(2.0*PI)/p ).sin());
                }
                else{
                    let t = t - 1.0;
                    return 0.5 * 2.0f64.powf(-10.0*t) * ( (t-s)*(2.0*PI)/p ).sin() + 1.0;
                }
            },
            Ease::InBack=>{
                let s = 1.70158; 
                return t * t * ((s+1.)*t - s);
            },
            Ease::OutBack=>{
                let s = 1.70158; 
                let t = t - 1.;
                return t * t * ((s+1.)*t + s) + 1.;
            },
            Ease::InOutBack=>{
                let s = 1.70158;
                let t = t * 2.0;
                if t < 1.{
                    let s = s * 1.525;
                    return 0.5 * (t * t * ((s+1.)*t - s));
                }
                else{
                    let t = t - 2.;
                    return 0.5 * (t * t * ((s+1.)*t + s) + 2.);
                }
            },
            Ease::InBounce=>{
                return 1.0 - Ease::OutBounce.map(1.0 - t);
            },
            Ease::OutBounce=>{
                if t < (1./2.75){
                    return 7.5625*t*t;
                }
                if t < (2./2.75){
                    let t = t - (1.5/2.75);
                    return 7.5625*t*t + 0.75;
                } 
                if t < (2.5/2.75){
                    let t = t - (2.25/2.75);
                    return 7.5625*t*t + 0.9375;
                }
                let t = t - (2.625/2.75);
                return 7.5625*t*t + 0.984375;
            },
            Ease::InOutBounce=>{
                if t <0.5{
                    return Ease::InBounce.map(t*2.)*0.5;
                }
                else{
                    return Ease::OutBounce.map(t*2. - 1.)*0.5+0.5;
                }
            },
            /* forgot the parameters to these functions
            Ease::Bounce{dampen}=>{
                if time < 0.{
                    return 0.;
                }
                if time > 1. {
                    return 1.;
                }

                let it = time * (1. / (1. - dampen)) + 0.5;
                let inlog = (dampen - 1.) * it + 1.0;
                if inlog <= 0. {
                    return 1.
                }
                let k = (inlog.ln() / dampen.ln()).floor();
                let d = dampen.powf(k);
                return 1. - (d * (it - (d - 1.) / (dampen - 1.)) - (it - (d - 1.) / (dampen - 1.)).powf(2.)) * 4.
            },
            Ease::Elastic{duration, frequency, decay, ease}=>{
                if time < 0.{
                    return 0.;
                }
                if time > 1. {
                    return 1.;
                }
                let mut easein = *ease;
                let mut easeout = 1.;
                if *ease < 0. {
                    easeout = -ease;
                    easein = 1.;
                }
                
                if time < *duration{
                    return Ease::Pow{begin:easein, end:easeout}.map(time / duration)
                }
                else {
                    // we have to snap the frequency so we end at 0
                    let w = ((0.5 + (1. - duration) * frequency * 2.).floor() / ((1. - duration) * 2.)) * std::f64::consts::PI * 2.;
                    let velo = (Ease::Pow{begin:easein, end:easeout}.map(1.001) - Ease::Pow{begin:easein, end:easeout}.map(1.) ) / (0.001 * duration);
                    return 1. + velo * ((((time - duration) * w).sin() / ((time - duration) * decay).exp()) / w)
                }
            },*/

            Ease::Bezier{cp0, cp1, cp2, cp3}=>{
                if t < 0.{
                    return 0.;
                }
                if t > 1. {
                    return 1.;
                }

                if (cp0 - cp1).abs() < 0.001 && (cp2 - cp3).abs() < 0.001{
                    return t;
                }
		
                let epsilon = 1.0 / 200.0 * t;
                let cx = 3.0 * cp0;
                let bx = 3.0 * (cp2 - cp0) - cx;
                let ax = 1.0 - cx - bx;
                let cy = 3.0 * cp1;
                let by = 3.0 * (cp3 - cp1) - cy;
                let ay = 1.0 - cy - by;
                let mut u = t;
                
                for _i in 0..6{
                    let x = ((ax * u + bx) * u + cx) * u - t;
                    if x.abs() < epsilon {
                        return ((ay * u + by) * u + cy) * u;
                    }
                    let d = (3.0 * ax * u + 2.0 * bx) * u + cx;
                    if d.abs() < 1e-6{
                        break;
                    }
                    u = u - x / d;
                };
                
                if t > 1.{
                    return (ay + by) + cy;
                }
                if t < 0.{
                    return 0.0;
                }
                
                let mut w = 0.0;
                let mut v = 1.0;
                u = t;
                for _i in 0..8{
                    let x = ((ax * u + bx) * u + cx) * u;
                    if (x - t).abs() < epsilon{
                        return ((ay * u + by) * u + cy) * u;
                    }
                    
                    if t > x{
                        w = u;
                    }
                    else{
                        v = u;
                    }
                    u = (v - w) * 0.5 + w;
                }
                
                return ((ay * u + by) * u + cy) * u;
            }
        }
    }
}

#[derive(Clone,Debug)]
pub struct FloatTrack{
    pub ident:String,
    pub ease:Ease,
    pub cut_init:Option<f32>,
    pub track:Vec<(f64, f32)>
}

#[derive(Clone,Debug)]
pub struct Vec2Track{
    pub ident:String,
    pub ease:Ease,
    pub cut_init:Option<Vec2>,
    pub track:Vec<(f64, Vec2)>
}

#[derive(Clone,Debug)]
pub struct Vec3Track{
    pub ident:String,
    pub ease:Ease,
    pub cut_init:Option<Vec3>,
    pub track:Vec<(f64, Vec3)>
}

#[derive(Clone,Debug)]
pub struct Vec4Track{
    pub ident:String,
    pub ease:Ease,
    pub cut_init:Option<Vec4>,
    pub track:Vec<(f64, Vec4)>
}

#[derive(Clone,Debug)]
pub struct ColorTrack{
    pub ident:String,
    pub ease:Ease,
    pub cut_init:Option<Color>,
    pub track:Vec<(f64, Color)>
}

#[derive(Clone,Debug)]
pub enum Track{
    Float(FloatTrack),
    Vec2(Vec2Track),
    Vec3(Vec3Track),
    Vec4(Vec4Track),
    Color(ColorTrack),
}

impl Track{
    pub fn float(ident:&str, ease:Ease, track:Vec<(f64,f32)>)->Track{
        Track::Float(FloatTrack{
            cut_init:None,
            ease:ease,
            ident:ident.to_string(),
            track:track
        })
    }
/*
    pub fn to_float(ident:&str, value:f32)->Track{
        Track::Float(FloatTrack{
            cut_init:None,
            ease:Ease::Linear,
            ident:ident.to_string(),
            track:vec![(1.0,value)]
        })
    }*/

    pub fn vec2(ident:&str, ease:Ease, track:Vec<(f64,Vec2)>)->Track{
        Track::Vec2(Vec2Track{
            cut_init:None,
            ease:ease,
            ident:ident.to_string(),
            track:track
        })
    }
    /*
    pub fn to_vec2(ident:&str, value:Vec2)->Track{
        Track::Vec2(Vec2Track{
            cut_init:None,
            ease:Ease::Linear,
            ident:ident.to_string(),
            track:vec![(1.0,value)]
        })
    }*/

    pub fn vec3(ident:&str, ease:Ease, track:Vec<(f64,Vec3)>)->Track{
        Track::Vec3(Vec3Track{
            cut_init:None,
            ease:ease,
            ident:ident.to_string(),
            track:track
        })
    }
    /*
    pub fn to_vec3(ident:&str, value:Vec3)->Track{
        Track::Vec3(Vec3Track{
            cut_init:None,
            ease:Ease::Linear,
            ident:ident.to_string(),
            track:vec![(1.0,value)]
        })
    }*/

    pub fn vec4(ident:&str, ease:Ease, track:Vec<(f64,Vec4)>)->Track{
        Track::Vec4(Vec4Track{
            cut_init:None,
            ease:ease,
            ident:ident.to_string(),
            track:track
        })
    }
        
    pub fn color(ident:&str, ease:Ease, track:Vec<(f64,Color)>)->Track{
        Track::Color(ColorTrack{
            cut_init:None,
            ease:ease,
            ident:ident.to_string(),
            track:track
        })
    }
    /*
    pub fn to_vec4(ident:&str, value:Vec4)->Track{
        Track::Vec4(Vec4Track{
            cut_init:None,
            ease:Ease::Linear,
            ident:ident.to_string(),
            track:vec![(1.0,value)]
        })
    }*/

    fn compute_track_value<T>(time:f64, track:&Vec<(f64,T)>, cut_init:&mut Option<T>, init:T, ease:&Ease) -> T
    where T:ComputeTrackValue<T> + Clone
    {
        if track.is_empty(){return init}
        // find the 2 keys we want
        for i in 0..track.len(){
            if time>= track[i].0{ // we found the left key
                let val1 = &track[i];
                if i == track.len() - 1{ // last key
                    return val1.1.clone()
                }
                let val2 = &track[i+1];
                // lerp it
                let f = ease.map( (time - val1.0)/(val2.0-val1.0) ) as f32;
                return val1.1.lerp_prop(&val2.1, f);
            }
        }
        if cut_init.is_none(){
            *cut_init = Some(init);
        }
        let val2 = &track[0];
        let val1 = cut_init.as_mut().unwrap();
        let f = ease.map( time/val2.0 ) as f32;
        return  val1.lerp_prop(&val2.1, f)
    }

    pub fn ident(&self)->&String{
        match self{
            Track::Float(ft)=>{
                &ft.ident
            },
            Track::Vec2(ft)=>{
                &ft.ident
            }
            Track::Vec3(ft)=>{
                &ft.ident
            }
            Track::Vec4(ft)=>{
                &ft.ident
            }
            Track::Color(ft)=>{
                &ft.ident
            }
        }
    }

    pub fn reset_cut_init(&mut self){
        match self{
            Track::Color(at)=>{
                at.cut_init = None;
            },
            Track::Vec4(at)=>{
                at.cut_init = None;
            },
            Track::Vec3(at)=>{
                at.cut_init = None;
            },
            Track::Vec2(at)=>{
                at.cut_init = None;
            },
            Track::Float(at)=>{
                at.cut_init = None;
            }
        }
    }

    pub fn ease(&self)->&Ease{
        match self{
            Track::Float(ft)=>{
                &ft.ease
            },
            Track::Vec2(ft)=>{
                &ft.ease
            }
            Track::Vec3(ft)=>{
                &ft.ease
            }
            Track::Vec4(ft)=>{
                &ft.ease
            }
            Track::Color(ft)=>{
                &ft.ease
            }
        }
    }
}

impl Anim{
    pub fn new(mode:Play, tracks:Vec<Track>)->Anim{
        Anim{
            mode:mode,
            tracks:tracks
        }
    }

    pub fn empty()->Anim{
        Anim{
            mode:Play::Cut{duration:0.},
            tracks:vec![]
        }
    }
}

#[derive(Clone,Debug)]
pub enum Play{
    Chain{duration:f64},
    Cut{duration:f64},
    Single{duration:f64, cut:bool, term:bool, end:f64},
    Loop{duration:f64, cut:bool, term:bool, repeats:f64,end:f64},
    Reverse{duration:f64, cut:bool, term:bool, repeats:f64,end:f64},
    Bounce{duration:f64, cut:bool, term:bool, repeats:f64,end:f64},
    Forever{duration:f64, cut:bool, term:bool},
    LoopForever{duration:f64, cut:bool, term:bool, end:f64},
    ReverseForever{duration:f64, cut:bool, term:bool, end:f64},
    BounceForever{duration:f64, cut:bool, term:bool, end:f64},
}

impl Play{
    pub fn duration(&self)->f64{
        match self{
            Play::Chain{duration,..}=>*duration,
            Play::Cut{duration,..}=>*duration,
            Play::Single{duration,..}=>*duration,
            Play::Loop{duration,..}=>*duration,
            Play::Reverse{duration,..}=>*duration,
            Play::Bounce{duration,..}=>*duration,
            Play::BounceForever{duration,..}=>*duration,
            Play::Forever{duration,..}=>*duration,
            Play::LoopForever{duration,..}=>*duration,
            Play::ReverseForever{duration,..}=>*duration,
        }
    }
    pub fn total_time(&self)->f64{
        match self{
            Play::Chain{duration,..}=>*duration,
            Play::Cut{duration,..}=>*duration,
            Play::Single{end,duration,..}=>end*duration,
            Play::Loop{end,duration,repeats,..}=>end*duration*repeats,
            Play::Reverse{end,duration,repeats,..}=>end*duration*repeats,
            Play::Bounce{end,duration,repeats,..}=>end*duration*repeats,
            Play::BounceForever{..}=>std::f64::INFINITY,
            Play::Forever{..}=>std::f64::INFINITY,
            Play::LoopForever{..}=>std::f64::INFINITY,
            Play::ReverseForever{..}=>std::f64::INFINITY,
        }
    }    

    pub fn cut(&self)->bool{
        match self{
            Play::Cut{..}=>true,
            Play::Chain{..}=>false,
            Play::Single{cut,..}=>*cut,
            Play::Loop{cut,..}=>*cut,
            Play::Reverse{cut,..}=>*cut,
            Play::Bounce{cut,..}=>*cut,
            Play::BounceForever{cut,..}=>*cut,
            Play::Forever{cut,..}=>*cut,
            Play::LoopForever{cut,..}=>*cut,
            Play::ReverseForever{cut,..}=>*cut,
        }
    }

    pub fn repeats(&self)->f64{
        match self{
            Play::Chain{..}=>1.0,
            Play::Cut{..}=>1.0,
            Play::Single{..}=>1.0,
            Play::Loop{repeats,..}=>*repeats,
            Play::Reverse{repeats,..}=>*repeats,
            Play::Bounce{repeats,..}=>*repeats,
            Play::BounceForever{..}=>std::f64::INFINITY,
            Play::Forever{..}=>std::f64::INFINITY,
            Play::LoopForever{..}=>std::f64::INFINITY,
            Play::ReverseForever{..}=>std::f64::INFINITY,
        }
    }

    pub fn term(&self)->bool{
        match self{
            Play::Cut{..}=>false,
            Play::Chain{..}=>false,
            Play::Single{term,..}=>*term,
            Play::Loop{term,..}=>*term,
            Play::Reverse{term,..}=>*term,
            Play::Bounce{term,..}=>*term,
            Play::BounceForever{term,..}=>*term,
            Play::Forever{term,..}=>*term,
            Play::LoopForever{term,..}=>*term,
            Play::ReverseForever{term,..}=>*term,
        }
    }

    pub fn compute_time(&self, time:f64)->f64{
        match self{
            Play::Cut{duration,..}=>{
                time / duration
            },
            Play::Chain{duration,..}=>{
                time / duration
            },
            Play::Single{duration,..}=>{
                time / duration
            },
            Play::Loop{end,duration,..}=>{
                (time / duration)  % end
            },
            Play::Reverse{end,duration,..}=>{
                end - (time / duration)  % end
            },
            Play::Bounce{end,duration,..}=>{ 
                let mut local_time = (time / duration)  % (end*2.0);
                if local_time > *end{
                    local_time = 2.0*end - local_time;
                };
                local_time
            },
            Play::BounceForever{end,duration,..}=>{
                let mut local_time = (time / duration)  % (end*2.0);
                if local_time > *end{
                    local_time = 2.0*end - local_time;
                };
                local_time
            },
            Play::Forever{duration,..}=>{
                let local_time = time / duration;
                local_time
            },
            Play::LoopForever{end, duration, ..}=>{
                let local_time = (time / duration)  % end;
                local_time
            },
            Play::ReverseForever{end, duration, ..}=>{
                let local_time = end - (time / duration)  % end;
                local_time
            },
        }
    }
}

trait ComputeTrackValue<T>{
    fn lerp_prop(&self, b:&T, f:f32)->T;
}

impl ComputeTrackValue<f32> for f32{
    fn lerp_prop(&self, b:&f32, f:f32)->f32{
        *self + (*b - *self) * f
    }
}

impl ComputeTrackValue<Vec2> for Vec2{
    fn lerp_prop(&self, b:&Vec2, f:f32)->Vec2{
        Vec2{
            x:self.x + (b.x - self.x) * f,
            y:self.y + (b.y - self.y) * f
        }
    }
}

impl ComputeTrackValue<Vec3> for Vec3{
    fn lerp_prop(&self, b:&Vec3, f:f32)->Vec3{
        Vec3{
            x:self.x + (b.x - self.x) * f,
            y:self.y + (b.y - self.y) * f,
            z:self.z + (b.z - self.z) * f
        }
    }
}

impl ComputeTrackValue<Vec4> for Vec4{
    fn lerp_prop(&self, b:&Vec4, f:f32)->Vec4{
        let of = 1.0-f;
        Vec4{
            x:self.x * of + b.x * f,
            y:self.y * of + b.y * f,
            z:self.z * of + b.z * f,
            w:self.w * of + b.w * f
        }
    }
}


impl ComputeTrackValue<Color> for Color{
    fn lerp_prop(&self, b:&Color, f:f32)->Color{
        let of = 1.0-f;
        Color{
            r:self.r * of + b.r * f,
            g:self.g * of + b.g * f,
            b:self.b * of + b.b * f,
            a:self.a * of + b.a * f
        }
    }
}