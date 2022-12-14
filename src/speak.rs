#![allow(non_upper_case_globals)]

use espeakng_sys::*;
use std::ffi::CString;
use std::os::raw::{c_int, c_short, c_void};

/// The name of the voice to use
pub const DEFAULT_VOICE: &str = "en";

pub struct Speak {
  voice: Option<String>,
  _marker: std::marker::PhantomData<std::cell::Cell<()>>,
}

type AudioStream = Vec<i16>;

unsafe extern "C" fn synth_callback(
  wav: *mut c_short,
  sample_count: c_int,
  mut events: *mut espeak_EVENT,
) -> c_int {
  if wav.is_null() {
    return 0;
  }

  let mut buffer = None;
  loop {
    if let Some(audio_buffer) = ((*events).user_data as *mut AudioStream).as_mut() {
      buffer = Some(audio_buffer)
    }

    if (*events).type_ == espeak_EVENT_TYPE_espeakEVENT_LIST_TERMINATED {
      break;
    }

    events = events.add(1);
  }

  if let Some(buffer) = buffer {
    if sample_count > 0 {
      buffer.extend_from_slice(std::slice::from_raw_parts(wav, sample_count as usize));
    }
  }

  0
}

impl Speak {
  /// Initialise espeak
  pub fn init() -> Self {
    unsafe {
      espeak_Initialize(espeak_AUDIO_OUTPUT_AUDIO_OUTPUT_SYNCHRONOUS, 0, std::ptr::null(), 0)
    };

    let voice = CString::new(DEFAULT_VOICE).expect("Failed to convert &str to CString");
    unsafe {
      if espeak_SetVoiceByName(voice.as_ptr()) != espeak_ERROR_EE_OK {
        panic!("Failed to set default voice");
      }

      espeak_SetSynthCallback(Some(synth_callback))
    }

    Speak { voice: Some(DEFAULT_VOICE.into()), _marker: std::marker::PhantomData }
  }

  fn sample_rate(&self) -> i32 {
    unsafe { espeak_ng_GetSampleRate() }
  }

  pub fn set_voice(&mut self, voice: &str) -> std::result::Result<i32, &'static str> {
    if let Some(current_voice) = &self.voice {
      if current_voice == voice {
        return Ok(self.sample_rate());
      }
    }

    log::info!("Setting voice to {}", voice);
    let c_voice = CString::new(voice).map_err(|_| "Invalid language name")?;
    let err = unsafe { espeak_SetVoiceByName(c_voice.as_ptr()) };

    let (res, new_voice) = match err {
      espeak_ERROR_EE_OK => (Ok(()), Some(voice.into())),
      espeak_ERROR_EE_NOT_FOUND => (Err("Unknown language"), None),
      _ => (Err("An unknown error occurred"), None),
    };
    self.voice = new_voice;

    res.map(|()| self.sample_rate())
  }

  pub fn speak(&mut self, text: &str) -> std::result::Result<AudioStream, &'static str> {
    let mut buf: AudioStream = Vec::new();
    let text_cstr = CString::new(text).map_err(|_| "Malformed input string")?;

    let mut err = unsafe {
      espeak_Synth(
        text_cstr.as_ptr() as *const c_void,
        0, // Unused with AUDIO_OUTPUT_SYNCHRONOUS
        0,
        espeak_POSITION_TYPE_POS_CHARACTER,
        0,
        espeakCHARS_AUTO,
        std::ptr::null_mut(),
        std::ptr::addr_of_mut!(buf) as *mut _,
      )
    };
    drop(text_cstr);

    if err == espeak_ERROR_EE_OK {
      err = unsafe { espeak_Synchronize() };
    }

    match err {
      espeak_ERROR_EE_OK => Ok(buf),
      espeak_ERROR_EE_BUFFER_FULL => Err("Buffer is full"),
      espeak_ERROR_EE_INTERNAL_ERROR => Err("Internal error"),
      espeak_ERROR_EE_NOT_FOUND => Err("Audio data not found"),
      _ => Err("An unknown error occurred"),
    }
  }
}

impl Drop for Speak {
  fn drop(&mut self) {
    unsafe {
      espeak_ng_Terminate();
    }
  }
}
