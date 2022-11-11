# CC: Speaks
A very silly TTS server using [espeak-ng](https://github.com/espeak-ng/espeak-ng/).

## Try it
Run the following command from a computer. Make sure you've got a speaker attached!

```
speaker play https://music.madefor.cc/tts?text=This%20%is%some%20text
```

You can also use this directly in your Lua code.

```lua
local message = "Just fiddling with text to speech."

local url = "https://music.madefor.cc/tts?text=" .. textutils.urlEncode(message)
local response, err = http.get { url = url, binary = true }
if not response then error(err, 0) end

local speaker = peripheral.find("speaker")
local decoder = require("cc.audio.dfpwm").make_decoder()

while true do
    local chunk = response.read(16 * 1024)
    if not chunk then break end

    local buffer = decoder(chunk)
    while not speaker.playAudio(buffer) do
        os.pullEvent("speaker_audio_empty")
    end
end
```
