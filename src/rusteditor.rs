use widgets::*;
use crate::textbuffer::*;
use crate::codeeditor::*;

#[derive(Clone)]
pub struct RustEditor{
    pub path:String,
    pub code_editor:CodeEditor,
}

impl ElementLife for RustEditor{
    fn construct(&mut self, _cx:&mut Cx){}
    fn destruct(&mut self, _cx:&mut Cx){}
}

impl Style for RustEditor{
    fn style(cx:&mut Cx)->Self{
        let rust_editor = Self{
            path:"".to_string(),
            code_editor:CodeEditor{
                ..Style::style(cx)
            },
        };
        //tab.animator.default = tab.anim_default(cx);
        rust_editor
    }
}

#[derive(Clone, PartialEq)]
pub enum RustEditorEvent{
    None,
    Change
}


impl RustEditor{
    pub fn handle_rust_editor(&mut self, cx:&mut Cx, event:&mut Event, text_buffer:&mut TextBuffer)->CodeEditorEvent{
        match self.code_editor.handle_code_editor(cx, event, text_buffer){
            _=>()
        }
        
        CodeEditorEvent::None
    }

    pub fn draw_rust_editor(&mut self, cx:&mut Cx, text_buffer:&TextBuffer){
        if let Err(()) = self.code_editor.begin_code_editor(cx, text_buffer){
            return
        }
        
        let mut state = TokenizerState::new(text_buffer);
       
        let mut looping = true;
        let mut chunk = Vec::new();
        while looping{
            let token_type;
            state.advance_with_cur();

            match state.cur{
                '\0'=>{ // eof insert a terminating space and end
                    token_type = TokenType::Whitespace;
                    chunk.push(' ');
                    looping = false;
                },
                '\n'=>{
                    token_type = TokenType::Newline;
                    chunk.push('\n');
                },
                ' ' | '\t'=>{ // eat as many spaces as possible
                    token_type = TokenType::Whitespace;
                    chunk.push(state.cur);
                    while state.next == ' '{
                        chunk.push(state.next);
                        state.advance();
                    }
                },
                '/'=>{ // parse comment
                    chunk.push(state.cur);
                    if state.next == '/'{
                        while state.next != '\n' && state.next != '\0'{
                            chunk.push(state.next);
                            state.advance();
                        }
                        token_type = TokenType::Comment;
                    }
                    else{
                        if state.next == '='{
                            chunk.push(state.next);
                            state.advance();
                        }
                        token_type = TokenType::Operator;
                    }
                },
                '\''=>{ // parse char literal or lifetime annotation
                    chunk.push(state.cur);

                    if Self::parse_rust_escape_char(&mut state, &mut chunk){ // escape char or unicode
                        if state.next == '\''{ // parsed to closing '
                            chunk.push(state.next);
                            state.advance();
                            token_type = TokenType::String;
                        }
                        else{
                            token_type = TokenType::Comment;
                        }
                    }
                    else{ // parse a single char or lifetime
                        let offset = state.offset;
                        if Self::parse_rust_ident_tail(&mut state, &mut chunk) && ((state.offset - offset) > 1 || state.next != '\''){
                            token_type = TokenType::Keyword;
                        }
                        else if state.next != '\n'{
                            if (state.offset - offset) == 0{ // not an identifier char
                                chunk.push(state.next);
                                state.advance();
                            }
                            if state.next == '\''{ // lifetime identifier
                                chunk.push(state.next);
                                state.advance();
                            }
                            token_type = TokenType::String;
                        }
                        else{
                            token_type = TokenType::String;
                        }
                    }
                },
                '"'=>{ // parse string
                    chunk.push(state.cur);
                    state.prev = '\0';
                    while state.next != '\0' && state.next!='\n' && (state.next != '"' || state.prev != '\\' && state.cur == '\\' && state.next == '"'){
                        chunk.push(state.next);
                        state.advance_with_prev();
                    };
                    chunk.push(state.next);
                    state.advance();
                    token_type = TokenType::String;
                   
                },
                '0'...'9'=>{ // try to parse numbers
                    token_type = TokenType::Number;
                    chunk.push(state.cur);
                    Self::parse_rust_number_tail(&mut state, &mut chunk);
                    
                },
                ':'=>{
                    chunk.push(state.cur);
                    if state.next == ':'{
                        chunk.push(state.next);
                        state.advance();
                    }
                    token_type = TokenType::Operator;
                },
                '*'=>{
                    chunk.push(state.cur);
                    if state.next == '='{
                        chunk.push(state.next);
                        state.advance();
                    }                    
                    token_type = TokenType::Operator;
                },
                '+'=>{
                    chunk.push(state.cur);
                    if state.next == '='{
                        chunk.push(state.next);
                        state.advance();
                    }
                    token_type = TokenType::Operator;
                },
                '-'=>{
                    chunk.push(state.cur);
                    if state.next == '>' || state.next == '='{
                        chunk.push(state.next);
                        state.advance();
                    }
                    token_type = TokenType::Operator;
                },
                '='=>{
                    chunk.push(state.cur);
                    if state.next == '>' {
                        chunk.push(state.next);
                        state.advance();
                    }
                    token_type = TokenType::Operator;
                },
                '.'=>{
                    chunk.push(state.cur);
                    if state.next == '.' {
                        chunk.push(state.next);
                        state.advance();
                    }
                    token_type = TokenType::Operator;
                },
                '(' | '{' | '['=>{
                    chunk.push(state.cur);
                    token_type = TokenType::ParenOpen;
                },
                ')' | '}' | ']'=>{
                    chunk.push(state.cur);
                    token_type = TokenType::ParenClose;
                },
                '_'=>{
                    chunk.push(state.cur);
                    Self::parse_rust_ident_tail(&mut state, &mut chunk);
                    token_type = TokenType::Identifier;
                },
                'a'...'z'=>{ // try to parse keywords or identifiers
                    chunk.push(state.cur);
                    let mut keyword_type = Self::parse_rust_lc_keyword(&mut state, &mut chunk);

                    if Self::parse_rust_ident_tail(&mut state, &mut chunk){
                        keyword_type = KeywordType::None;
                    }
                    match keyword_type{
                        KeywordType::Normal=>{
                            token_type = TokenType::Keyword;
                        },
                        KeywordType::Flow=>{
                            token_type = TokenType::Flow;
                        },
                        KeywordType::None=>{
                            if state.next == '(' || state.next == '!'{
                                token_type = TokenType::Call;
                            }
                            else{
                                token_type = TokenType::Identifier;
                            }
                        }
                    }
                },
                'A'...'Z'=>{
                    chunk.push(state.cur);
                    let mut is_keyword = false;
                    if state.cur == 'S'{
                        if state.keyword(&mut chunk, "elf"){
                            is_keyword = true;
                        }
                    }
                    if Self::parse_rust_ident_tail(&mut state, &mut chunk){
                        is_keyword = false;
                    }
                    if is_keyword{
                        token_type = TokenType::Keyword;
                    }
                    else{
                        token_type = TokenType::TypeName;
                    }
                },
                _=>{
                    chunk.push(state.cur);
                    token_type = TokenType::Operator;
                }
            }
            let off = state.offset - chunk.len() - 1;
            self.code_editor.draw_chunk(cx, &chunk, off, token_type);
            chunk.truncate(0);
        }
        
        self.code_editor.end_code_editor(cx, text_buffer);
    }

    fn parse_rust_ident_tail<'a>(state:&mut TokenizerState<'a>, chunk:&mut Vec<char>)->bool{
        let mut ret = false;
        while state.next_is_digit() || state.next_is_letter() || state.next == '_' || state.next == '$'{
            ret = true;
            chunk.push(state.next);
            state.advance();
        }
        ret
    }

    fn parse_rust_escape_char<'a>(state:&mut TokenizerState<'a>, chunk:&mut Vec<char>)->bool{
        if state.next == '\\'{
            chunk.push(state.next);
            state.advance();
            if state.next == 'u'{
                chunk.push(state.next);
                state.advance();
                if state.next == '{'{
                    chunk.push(state.next);
                    state.advance();
                    while state.next_is_hex(){
                        chunk.push(state.next);
                        state.advance();
                    }
                    if state.next == '}'{
                        chunk.push(state.next);
                        state.advance();
                    }
                }
            }
            else{
                // its a single char escape TODO limit this to valid escape chars
                chunk.push(state.next);
                state.advance();
            }
            return true
        }
        return false
    }

    fn parse_rust_number_tail<'a>(state:&mut TokenizerState<'a>, chunk:&mut Vec<char>){
        if state.next == 'x'{ // parse a hex number
            chunk.push(state.next);
            state.advance();
            while state.next_is_hex() || state.next == '_'{
                chunk.push(state.next);
                state.advance();
            }
        }
        else if state.next == 'b'{ // parse a binary
            chunk.push(state.next);
            state.advance();
            while state.next == '0' || state.next =='1' || state.next == '_'{
                chunk.push(state.next);
                state.advance();
            }
        }
        else{
            while state.next_is_digit() || state.next == '_'{
                chunk.push(state.next);
                state.advance();
            }
            if state.next == 'u' || state.next == 'i'{
                chunk.push(state.next);
                state.advance();
                if state.keyword(chunk, "8"){
                }
                else if state.keyword(chunk, "16"){
                }
                else if state.keyword(chunk,"32"){
                }
                else if state.keyword(chunk,"64"){
                }
            }
            else if state.next == '.'{
                chunk.push(state.next);
                state.advance();
                // again eat as many numbers as possible
                while state.next_is_digit() || state.next == '_'{
                    chunk.push(state.next);
                    state.advance();
                }
                if state.next == 'f' { // the f32, f64 postfix
                    chunk.push(state.next);
                    state.advance();
                    if state.keyword(chunk,"32"){
                    }
                    else if state.keyword(chunk,"64"){
                    }
                }
            }
        }
    }

    fn parse_rust_lc_keyword<'a>(state:&mut TokenizerState<'a>, chunk:&mut Vec<char>)->KeywordType{
        match state.cur{
            'a'=>{
                if state.keyword(chunk,"s"){
                    return KeywordType::Normal
                }
            },
            'b'=>{ 
                if state.keyword(chunk,"reak"){
                    return KeywordType::Flow
                }
            },
            'c'=>{
                if state.keyword(chunk,"o"){
                    if state.keyword(chunk,"nst"){
                        return KeywordType::Normal
                    }
                    else if state.keyword(chunk,"ntinue"){
                        return KeywordType::Flow
                    }
                }
                else if state.keyword(chunk,"rate"){
                    return KeywordType::Normal
                }
            },
            'e'=>{
                if state.keyword(chunk,"lse"){
                    return KeywordType::Flow
                }
                else if state.keyword(chunk,"num"){
                    return KeywordType::Normal
                }
                else if state.keyword(chunk,"xtern"){
                    return KeywordType::Normal
                }
            },
            'f'=>{
                if state.keyword(chunk,"alse"){
                    return KeywordType::Normal
                }
                else if state.keyword(chunk,"n"){
                    return KeywordType::Normal
                }
                else if state.keyword(chunk,"or"){
                    return KeywordType::Flow
                }
            },
            'i'=>{
                if state.keyword(chunk,"f"){
                    return KeywordType::Flow
                }
                else if state.keyword(chunk,"mpl"){
                    return KeywordType::Normal
                }
                else if state.keyword(chunk,"in"){
                    return KeywordType::Normal
                }
            },
            'l'=>{
                if state.keyword(chunk,"et"){
                    return KeywordType::Normal
                }
                else if state.keyword(chunk,"oop"){
                    return KeywordType::Flow
                }
            },
            'm'=>{
                if state.keyword(chunk,"atc"){
                    return KeywordType::Flow
                }
                else if state.keyword(chunk,"o"){
                    if state.keyword(chunk,"d"){
                        return KeywordType::Normal
                    }
                    else if state.keyword(chunk,"ve"){
                        return KeywordType::Normal
                    }
                }
                else if state.keyword(chunk,"ut"){
                    return KeywordType::Normal
                }
            },
            'p'=>{ // pub
                if state.keyword(chunk,"ub"){ 
                    return KeywordType::Normal
                }
            },
            'r'=>{
                if state.keyword(chunk,"e"){
                    if state.keyword(chunk,"f"){
                        return KeywordType::Normal
                    }
                    else if state.keyword(chunk,"turn"){
                        return KeywordType::Flow
                    }
                }
            },
            's'=>{
                if state.keyword(chunk,"elf"){
                    return KeywordType::Normal
                }
                if state.keyword(chunk,"uper"){
                    return KeywordType::Normal
                }
                else if state.keyword(chunk,"t"){
                    if state.keyword(chunk,"atic"){
                        return KeywordType::Normal
                    }
                    else if state.keyword(chunk,"ruct"){
                        return KeywordType::Normal
                    }
                }
            },
            't'=>{
                if state.keyword(chunk,"ype"){
                    return KeywordType::Normal
                }
                else if state.keyword(chunk,"r"){
                    if state.keyword(chunk,"rait"){
                        return KeywordType::Normal
                    }
                    else if state.keyword(chunk,"ue"){
                        return KeywordType::Normal
                    }
                }
            },
            'u'=>{ // use
                if state.keyword(chunk,"se"){ 
                    return KeywordType::Normal
                }
                else if state.keyword(chunk,"nsafe"){ 
                    return KeywordType::Normal
                }
            },
            'w'=>{ // use
                if state.keyword(chunk,"h"){
                    if state.keyword(chunk,"ere"){
                        return KeywordType::Normal
                    }
                    else if state.keyword(chunk,"ile"){
                        return KeywordType::Flow
                    }
                }
            }, 
            _=>{}
        }     
        KeywordType::None
    }
}

enum KeywordType{
    None,
    Normal,
    Flow,
}