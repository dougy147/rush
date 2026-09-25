use MAX_SEARCH_LEN;
    
pub struct User_Search {
    pub bytes: [u8;MAX_SEARCH_LEN],
    pub size: usize,
    pub cursor: usize, // cursor position
}

/* streams */
impl User_Search {
    pub fn new() -> Self {
        Self {
            bytes: [0;MAX_SEARCH_LEN],
            size: 0,
            cursor: 0,
        }        
    }
    
    pub fn insert_key(&mut self, key: u8) {
        if self.cursor != self.size {
            // todo: assert no overflow
            for i in (self.cursor..self.size).rev() {
                self.bytes[i+1] = self.bytes[i];
            }        
        }
        self.bytes[self.cursor] = key;
        self.size += 1;
        self.cursor += 1;
        self.bytes[self.size] = b'\0';
    }

    pub fn delete_backward(&mut self) {
        // NOTE: we need to null terminate '\0'
        if self.cursor > 0 {
            for i in self.cursor..self.size {
                self.bytes[i-1] = self.bytes[i];
            }
            self.size -= 1;
            self.cursor -= 1;
            self.bytes[self.size] = b'\0';
        }
    }

    pub fn delete_from_cursor_to_bol(&mut self) {
        // NOTE: we need to null terminate '\0'
        if self.cursor > 0 {
            for i in self.cursor..self.size {
                self.bytes[i-self.cursor] = self.bytes[i];
            }
            self.size -= self.cursor;
            self.cursor = 0;
            for i in self.size..MAX_SEARCH_LEN {
                self.bytes[i] = b'\0';
            }
        }
    }

    pub fn delete_from_cursor_to_eol(&mut self) {
        // NOTE: we need to null terminate '\0'
        if self.cursor < self.size {
            for i in self.cursor..self.size {
                self.bytes[i] = b'\0';
            }
            self.size -= self.size - self.cursor;
        }
    }

    pub fn cursor_forward(&mut self) {
        if self.cursor < self.size {
            self.cursor += 1;
        }
    }

    pub fn cursor_backward(&mut self) {
        if self.cursor > 0 {
            self.cursor -= 1;
        }
    }

    pub fn cursor_to_bol(&mut self) {
        self.cursor = 0;
    }

    pub fn cursor_to_eol(&mut self) {
        self.cursor = self.size;
    }

    pub fn word_forward(&mut self) {
        // special chars to ignore: ' ', '"', '\''
        let specs = [b' ', b'"', b'\''];
        while self.cursor < self.size && specs.contains(&self.bytes[self.cursor]) {
            self.cursor += 1;
        }
        while self.cursor < self.size && !specs.contains(&self.bytes[self.cursor]) {
            self.cursor += 1;
        }
    }

    pub fn word_backward(&mut self) -> () {
        // if cursor at end of string, force one backward
        if self.cursor > 0 { //&& self.cursor == self.size {
            self.cursor -= 1;
        }

        // special chars to ignore: ' ', '"', '\''
        let specs = [b' ', b'"', b'\''];
        while self.cursor > 0 && specs.contains(&self.bytes[self.cursor]) {
            self.cursor -= 1;
        }
        while self.cursor > 0 && !specs.contains(&self.bytes[self.cursor]) {
            self.cursor -= 1;
        }
        if self.cursor < self.size && specs.contains(&self.bytes[self.cursor]) {
            if !specs.contains(&self.bytes[self.cursor+1]) {
                self.cursor += 1;
            }
        }
    }

    pub fn delete_word_backward(&mut self) -> () {
        // if cursor at end of string, force one backward
        let specs = [b' ', b'"', b'\''];

        while self.size > 0 && self.cursor > 0 && specs.contains(&self.bytes[self.cursor-1])  {
            for i in self.cursor-1..self.size-1 {
                self.bytes[i] = self.bytes[i+1];
            }
            self.cursor -= 1;
            self.size -= 1;
            self.bytes[self.size] = b'\0';
        }

        // specs stopping going further back
        while self.size > 0 && self.cursor > 0 && !specs.contains(&self.bytes[self.cursor-1]) {
            for i in self.cursor-1..self.size-1 {
                self.bytes[i] = self.bytes[i+1];
            }
            self.cursor -= 1;
            self.size -= 1;
            self.bytes[self.size] = b'\0';
        }
    }
}
