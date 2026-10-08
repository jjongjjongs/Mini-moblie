// The emulator, as the iOS app calls it. Implemented by the `wie_ios` crate.
//
// One title runs at a time. `wie_start`, `wie_tick`, `wie_take_frame` and the
// key and stop calls may come from any one thread at a time; `wie_render_audio`
// is meant for the audio thread and does not wait on a tick.

#ifndef WIE_H
#define WIE_H

#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>

// Key indexes, as the Android keypad lays them out.
enum {
    WIE_KEY_UP = 0,
    WIE_KEY_DOWN = 1,
    WIE_KEY_LEFT = 2,
    WIE_KEY_RIGHT = 3,
    WIE_KEY_OK = 4,
    WIE_KEY_LEFT_SOFT = 5,
    WIE_KEY_RIGHT_SOFT = 6,
    WIE_KEY_CLEAR = 7,
    WIE_KEY_NUM0 = 8, // NUM1..NUM9 follow as 9..17
    WIE_KEY_STAR = 18,
    WIE_KEY_HASH = 19,
    WIE_KEY_CALL = 20,
    WIE_KEY_HANGUP = 21,
};

// Loads `data` (a handset archive or a jar) and starts it, keeping its files
// under `runtime_dir`. Returns NULL on success, otherwise a message to show,
// to be freed with `wie_free_string`.
char *wie_start(const uint8_t *data, size_t length, const char *runtime_dir, const char *model);

// Runs the emulator for up to `budget_ms`. Returns NULL while the title is
// healthy, otherwise the message that stopped it (free with `wie_free_string`).
char *wie_tick(uint32_t budget_ms);

void wie_stop(void);
bool wie_running(void);

// What stopped the last run, or NULL if nothing did (or the title ended
// itself). Free with `wie_free_string`.
char *wie_last_error(void);

// How long the caller may sleep before the title has work again, or -1.
int64_t wie_sleep_hint_ms(void);

void wie_key(int32_t index, bool pressed);

// Copies the newest frame since the last call into `rgba` (`capacity` bytes)
// and stores its size. Returns false when nothing new was painted or `rgba` is
// too small; `*width`/`*height` are set either way when there is a frame.
bool wie_take_frame(uint8_t *rgba, size_t capacity, uint32_t *width, uint32_t *height);

// Fills `samples` with up to `frames` stereo frames of 44.1kHz 16-bit audio,
// interleaved. Returns how many frames were written; 0 while nothing sounds.
size_t wie_render_audio(int16_t *samples, size_t frames);

// Pops the next vibration request. Returns false when there is none.
bool wie_take_vibration(uint32_t *duration_ms);

// The log collected for this run. Free with `wie_free_string`.
char *wie_log(void);

void wie_free_string(char *string);

#endif
