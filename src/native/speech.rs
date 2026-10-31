//! Read aloud with the voices Windows already has (SAPI). Speaking runs on SAPI's own thread;
//! the reader asks each frame which word is being said and moves on when a paragraph is done.

use windows::core::HSTRING;
use windows::Win32::Media::Speech::{ISpVoice, SpVoice, SPF_ASYNC, SPF_IS_NOT_XML, SPF_PURGEBEFORESPEAK, SPRS_DONE, SPVOICESTATUS};
use windows::Win32::System::Com::{CoCreateInstance, CoInitializeEx, CLSCTX_ALL, COINIT_APARTMENTTHREADED};

pub struct Voice {
    voice: ISpVoice,
    /// The text being spoken, for turning SAPI's UTF-16 positions into byte offsets.
    text: String,
    /// Count of utterances handed to SAPI; status refers to the last one.
    stream: u32,
}

/// Where speaking has got to.
pub enum Progress {
    /// Saying the word at this byte range of the text.
    Word(usize, usize),
    /// Started, but no word reached yet.
    Starting,
    Done,
}

impl Voice {
    pub fn new() -> Option<Voice> {
        unsafe {
            // Harmless if COM is already set up on this thread.
            let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
            let voice: ISpVoice = CoCreateInstance(&SpVoice, None, CLSCTX_ALL).ok()?;
            Some(Voice { voice, text: String::new(), stream: 0 })
        }
    }

    /// Starts saying `text` (stopping anything already being said). `rate` is 1.0 for the
    /// voice's normal pace; SAPI's −10…10 scale runs from a third of it to three times it.
    pub fn speak(&mut self, text: &str, rate: f32) {
        let sapi_rate = (10.0 * rate.max(0.1).ln() / 3f32.ln()).round().clamp(-10.0, 10.0) as i32;
        unsafe {
            let _ = self.voice.SetRate(sapi_rate);
            let mut stream = 0u32;
            let flags = (SPF_ASYNC.0 | SPF_PURGEBEFORESPEAK.0 | SPF_IS_NOT_XML.0) as u32;
            if self.voice.Speak(&HSTRING::from(text), flags, Some(&mut stream)).is_ok() {
                self.stream = stream;
            }
        }
        self.text = text.to_string();
    }

    pub fn stop(&mut self) {
        unsafe {
            let flags = (SPF_ASYNC.0 | SPF_PURGEBEFORESPEAK.0) as u32;
            let _ = self.voice.Speak(&HSTRING::new(), flags, None);
        }
        self.text.clear();
    }

    pub fn progress(&self) -> Progress {
        let mut st = SPVOICESTATUS::default();
        if unsafe { self.voice.GetStatus(&mut st, std::ptr::null_mut()) }.is_err() {
            return Progress::Done;
        }
        if st.dwRunningState == SPRS_DONE.0 as u32 && st.ulCurrentStream >= self.stream {
            return Progress::Done;
        }
        if st.ulInputWordLen == 0 {
            return Progress::Starting;
        }
        // UTF-16 positions to byte offsets.
        let (pos, len) = (st.ulInputWordPos as usize, st.ulInputWordLen as usize);
        let mut units = 0;
        let (mut start, mut end) = (None, self.text.len());
        for (i, ch) in self.text.char_indices() {
            if units == pos {
                start = Some(i);
            }
            if units == pos + len {
                end = i;
                break;
            }
            units += ch.len_utf16();
        }
        match start {
            Some(s) => Progress::Word(s, end.max(s)),
            None => Progress::Starting,
        }
    }
}

impl Drop for Voice {
    /// Closing the book stops the voice mid-sentence.
    fn drop(&mut self) {
        self.stop();
    }
}
