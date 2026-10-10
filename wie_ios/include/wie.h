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
// As wie_take_frame, the frame doubled through hq2x: twice the title's width
// and height, its edges smoothed rather than its pixels made into blocks.
bool wie_take_frame_hq2x(uint8_t *rgba, size_t capacity, uint32_t *width, uint32_t *height);
// Has the next wie_take_frame* hand over the last frame again, though the
// title paints nothing new - so a change of quality shows on a still screen.
void wie_show_frame_again(void);
// Copies the last frame taken, as the title drew it, without taking it.
// Returns false when there is none or it does not fit in `capacity` bytes.
bool wie_last_frame(uint8_t *rgba, size_t capacity, uint32_t *width, uint32_t *height);
// Doubles `rgba` (`width` by `height`) through hq2x into `out`, which holds
// `width * height * 16` bytes. Returns false when it cannot.
bool wie_hq2x(const uint8_t *rgba, uint32_t width, uint32_t height, uint8_t *out);

// Fills `samples` with up to `frames` stereo frames of 44.1kHz 16-bit audio,
// interleaved. Returns how many frames were written; 0 while nothing sounds.
size_t wie_render_audio(int16_t *samples, size_t frames);

// Pops the next vibration request. Returns false when there is none.
bool wie_take_vibration(uint32_t *duration_ms);

// The carrier `data` runs under: "KTF", "LGT", "SKT", "DRM" (a locked
// download) or "". Free with `wie_free_string`; never NULL.
char *wie_carrier(const uint8_t *data, size_t length);

// How fast the title runs, 1.0 being real time.
void wie_set_speed(float speed);
float wie_speed(void);

// Saves, in the Android app's zip layout (db/<product id>/..., fs/<app id>/...),
// so a save moves between the two. `data` is the game file whose saves are
// meant; `runtime_dir` is the one given to `wie_start`. Each returns NULL on
// success, otherwise a message (free with `wie_free_string`).
//
// Writes the zip to `destination`; `*exported` is false when nothing is saved.
char *wie_export_save(const uint8_t *data, size_t length, const char *runtime_dir, const char *destination, bool *exported);
// Restores a save zip, overwriting; `*restored` is how many files.
char *wie_import_save(const uint8_t *zip, size_t length, const char *runtime_dir, size_t *restored);
// What a save zip is to the game `data`: -1 no save zip, 1 its saves, 0 another
// game's; `*files` and `*size` are how many saved files it holds and their bytes.
int32_t wie_save_zip_info(const uint8_t *zip, size_t zip_length, const uint8_t *data, size_t length, size_t *files, uint64_t *size);
// Removes the game's saves; `*removed` is how many directories.
char *wie_erase_save(const uint8_t *data, size_t length, const char *runtime_dir, size_t *removed);

// A touch on the game screen, at x, y in the frame's own pixels (as
// wie_take_frame hands it): action 0 down, 1 up, 2 move. Dropped unless touch
// is on.
void wie_pointer(int32_t action, int32_t x, int32_t y);

// Turns touches on the game screen on or off, and with them what the title is
// told when it asks for a touch screen. Safe while a title runs.
void wie_set_touch(bool enabled);
bool wie_touch(void);

// The log collected for this run. Free with `wie_free_string`.
char *wie_log(void);

void wie_free_string(char *string);

#endif
