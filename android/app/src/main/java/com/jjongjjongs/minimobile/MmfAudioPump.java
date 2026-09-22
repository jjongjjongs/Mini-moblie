package com.jjongjjongs.minimobile;

import android.content.Context;
import android.media.AudioAttributes;
import android.media.AudioFormat;
import android.media.AudioManager;
import android.media.AudioTrack;
import android.os.Build;
import android.os.Process;
import android.util.Log;

/**
 * Pulls the synthesiser's output on a dedicated thread, clocked by its own
 * AudioTrack, the way the reference player does.
 *
 * <p>The emulator used to push audio in bursts from the game loop - one chunk
 * per tick, tens of milliseconds apart at a low frame rate - and the stream
 * broke up between the bursts. Here a thread of its own asks the native side for
 * the next chunk ({@link NativeBridge#nativeRenderAudio}) and writes it with a
 * blocking write: the AudioTrack only accepts data as fast as it plays it, so
 * the write itself paces the render at real time, independent of how fast or
 * slow the game is running.
 */
final class MmfAudioPump {
    private static final String TAG = "WIE-MmfPump";

    private static final int RATE = 44100;
    private static final int CHANNELS = 2;
    private static final int FRAME_BYTES = CHANNELS * 2;
    /**
     * Frames asked for per pull, matched to the device's own mixer burst by
     * {@link #configure(Context)}.
     *
     * <p>The chunk bounds two separate waits, and both are what a sound started
     * over music has to sit through: a note reaching the mixer part way through
     * a chunk is not rendered until the next one begins, and the write carrying
     * it overshoots {@link #LEAD_MS} by however much a chunk holds.
     *
     * <p>Picking a number outright is the wrong instrument for it. The track
     * drains in whole HAL bursts and {@link AudioTrack#getPlaybackHeadPosition()}
     * only moves when one is consumed, so a chunk that is not a burst leaves the
     * queue reading below by a ragged remainder, and the lead gate above either
     * holds a write it should have sent or sends one it should have held. That
     * is jitter on every note rather than a fixed delay. Writing the device's
     * own burst puts every write on a boundary the reading can actually see.
     *
     * <p>The burst the device reports is in frames at its native output rate, so
     * it is scaled to {@link #RATE}, which is what this track runs at.
     */
    private static final int DEFAULT_CHUNK_FRAMES = 256;
    private static final int MIN_CHUNK_FRAMES = 96;
    private static final int MAX_CHUNK_FRAMES = 2048;
    /** Used until {@link #configure(Context)} reports the device's burst. */
    private static volatile int chunkFrames = DEFAULT_CHUNK_FRAMES;

    /** Track buffer, matching the reference's ~120 ms with headroom. */
    private static final int TRACK_BUFFER_MS = 180;
    /** Buffer filled before playback starts, so the first writes cannot drain
     *  the track before the pace settles. Below the buffer so a stopped track
     *  never blocks a write forever. */
    private static final int PREFILL_MS = 60;
    /**
     * How far ahead of the playback head this will render.
     *
     * <p>A blocking write on its own fills the track to the brim, so while
     * music plays the queue sits near {@link #TRACK_BUFFER_MS} and a sound
     * started now is mixed into the next chunk - behind all of it. That is the
     * effect arriving late over background music: the note was on time, the
     * audio in front of it was not.
     *
     * <p>This is also the whole of the margin. The track being large does not
     * help on its own - capacity absorbs nothing while it is empty - so what
     * stands between a late render and a break in the stream is exactly the
     * audio already queued, which is this. Lowering it buys latency straight
     * out of that margin, which is why the chunk came down instead.
     */
    private static final int LEAD_MS = 60;
    /** Consecutive waits before the lead is ignored and a write goes out
     *  anyway, so a device whose playback head does not advance the way this
     *  reads it falls back to the blocking write rather than to silence. */
    private static final int MAX_WAITS = 24;

    /** Emitted once a second so the queue ahead of a note can be read off a
     *  device log rather than reasoned about. */
    private static final long STATS_PERIOD_NS = 1_000_000_000L;

    private static Thread thread;
    private static volatile boolean running;
    private static volatile boolean paused;

    private MmfAudioPump() {}

    /**
     * Takes the device's native output burst and sample rate and sizes the
     * chunk from them. Safe to call more than once; falls back to
     * {@link #DEFAULT_CHUNK_FRAMES} when the device reports nothing.
     */
    static void configure(Context context) {
        int burst = 0;
        int nativeRate = 0;
        try {
            AudioManager manager = (AudioManager) context.getSystemService(Context.AUDIO_SERVICE);
            if (manager != null) {
                burst = parseProperty(manager.getProperty(AudioManager.PROPERTY_OUTPUT_FRAMES_PER_BUFFER));
                nativeRate = parseProperty(manager.getProperty(AudioManager.PROPERTY_OUTPUT_SAMPLE_RATE));
            }
        } catch (RuntimeException ignored) {
        }
        int chunk = DEFAULT_CHUNK_FRAMES;
        if (burst > 0) {
            long scaled = nativeRate > 0 ? (long) burst * RATE / nativeRate : burst;
            chunk = (int) Math.max(MIN_CHUNK_FRAMES, Math.min(MAX_CHUNK_FRAMES, scaled));
        }
        chunkFrames = chunk;
        report("[pump] device burst=" + burst + " @" + nativeRate + "Hz"
                + " -> chunk=" + chunk + " frames (" + (chunk * 1000 / RATE) + "ms @" + RATE + "Hz)");
    }

    private static int parseProperty(String value) {
        if (value == null) {
            return 0;
        }
        try {
            return Integer.parseInt(value.trim());
        } catch (NumberFormatException e) {
            return 0;
        }
    }

    static synchronized void start() {
        paused = false;
        if (thread != null && thread.isAlive()) {
            return;
        }
        running = true;
        thread = new Thread(MmfAudioPump::run, "WIE MMF audio");
        thread.setDaemon(true);
        thread.start();
    }

    static void pause() {
        paused = true;
    }

    static void resume() {
        paused = false;
    }

    static synchronized void release() {
        running = false;
        Thread old = thread;
        thread = null;
        if (old != null) {
            old.interrupt();
            try {
                old.join(250);
            } catch (InterruptedException ignored) {
            }
        }
    }

    private static void run() {
        Process.setThreadPriority(Process.THREAD_PRIORITY_URGENT_AUDIO);
        AudioTrack track = null;
        int prefillFrames = 0;
        boolean playing = false;
        long framesWritten = 0;
        int waits = 0;
        long statsStart = System.nanoTime();
        int statPulls = 0;
        int statWaits = 0;
        int statForced = 0;
        int statIdle = 0;
        int statQueuedSamples = 0;
        long statQueuedSum = 0;
        long statQueuedMin = Long.MAX_VALUE;
        long statQueuedMax = 0;
        int lastUnderruns = 0;
        try {
            while (running) {
                if (paused) {
                    if (track != null && playing && track.getPlayState() == AudioTrack.PLAYSTATE_PLAYING) {
                        try {
                            track.pause();
                        } catch (RuntimeException ignored) {
                        }
                    }
                    playing = false;
                    sleep(20);
                    continue;
                }

                // Far enough ahead already? Then let the track drain before
                // rendering more, so what is queued in front of a sound that
                // starts now stays near LEAD_MS. Checked before the pull, so
                // the synthesiser is not run ahead of playback either.
                if (track != null && playing) {
                    // A head still at zero has not started moving - play() has
                    // been called but the track has yet to pick it up. Waiting
                    // on that reading holds off the writes while the prefill
                    // plays out, which empties the track at the very moment a
                    // title's first sound is starting. Nothing is ahead of the
                    // playback yet either, so there is nothing to wait for.
                    //
                    // A count outside the track's own capacity is not a reading
                    // to act on. Either way this falls through to the blocking
                    // write.
                    long head = track.getPlaybackHeadPosition() & 0xFFFFFFFFL;
                    long queued = framesWritten - head;
                    long lead = (long) RATE * LEAD_MS / 1000;
                    long capacity = (long) RATE * TRACK_BUFFER_MS / 1000;
                    if (waits < MAX_WAITS && head > 0 && queued > lead && queued <= capacity) {
                        waits++;
                        statWaits++;
                        sleep(4);
                        continue;
                    }
                    if (waits >= MAX_WAITS) {
                        statForced++;
                    }
                    // The queue as it stands for the chunk about to be rendered:
                    // exactly what a note starting now waits through.
                    statQueuedSamples++;
                    statQueuedSum += queued;
                    statQueuedMin = Math.min(statQueuedMin, queued);
                    statQueuedMax = Math.max(statQueuedMax, queued);
                }
                waits = 0;

                int chunk = chunkFrames;
                byte[] pcm;
                try {
                    pcm = NativeBridge.nativeRenderAudio(chunk);
                } catch (Throwable t) {
                    pcm = null;
                }

                if (pcm == null || pcm.length == 0) {
                    // Nothing is sounding; idle briefly and keep the track ready.
                    statIdle++;
                    sleep(5);
                    continue;
                }

                if (track == null) {
                    track = openTrack();
                    prefillFrames = 0;
                    framesWritten = 0;
                    playing = false;
                    if (track == null) {
                        sleep(20);
                        continue;
                    }
                }

                // Blocking write: this is the clock. It returns only as the track
                // drains, so the render keeps pace with playback rather than the
                // game loop.
                int offset = 0;
                boolean ok = true;
                while (offset < pcm.length) {
                    int written = track.write(pcm, offset, pcm.length - offset, AudioTrack.WRITE_BLOCKING);
                    if (written <= 0) {
                        ok = false;
                        break;
                    }
                    offset += written;
                }
                if (!ok) {
                    closeTrack(track);
                    track = null;
                    playing = false;
                    framesWritten = 0;
                    continue;
                }
                framesWritten += pcm.length / FRAME_BYTES;
                statPulls++;

                long now = System.nanoTime();
                if (now - statsStart >= STATS_PERIOD_NS) {
                    int underruns = track.getUnderrunCount();
                    long span = Math.max(1L, now - statsStart);
                    report("[pump] " + (statPulls * 1_000_000_000L / span) + " pull/s"
                            + " chunk=" + chunk + "f"
                            + " queued=" + framesToMs(statQueuedSamples > 0 ? statQueuedMin : 0)
                            + "/" + framesToMs(statQueuedSamples > 0 ? statQueuedSum / statQueuedSamples : 0)
                            + "/" + framesToMs(statQueuedMax) + "ms"
                            + " waits=" + statWaits
                            + " forced=" + statForced
                            + " idle=" + statIdle
                            + " underruns=+" + (underruns - lastUnderruns));
                    lastUnderruns = underruns;
                    statsStart = now;
                    statPulls = 0;
                    statWaits = 0;
                    statForced = 0;
                    statIdle = 0;
                    statQueuedSamples = 0;
                    statQueuedSum = 0;
                    statQueuedMin = Long.MAX_VALUE;
                    statQueuedMax = 0;
                }

                if (!playing) {
                    prefillFrames += pcm.length / FRAME_BYTES;
                    if (prefillFrames >= RATE * PREFILL_MS / 1000) {
                        try {
                            track.play();
                            playing = true;
                        } catch (RuntimeException ignored) {
                        }
                    }
                }
            }
        } catch (RuntimeException e) {
            Log.e(TAG, "audio pump failed", e);
        } finally {
            closeTrack(track);
        }
    }

    private static AudioTrack openTrack() {
        int mask = AudioFormat.CHANNEL_OUT_STEREO;
        int minimum = AudioTrack.getMinBufferSize(RATE, mask, AudioFormat.ENCODING_PCM_16BIT);
        if (minimum <= 0) {
            return null;
        }
        int bufferBytes = Math.max(minimum, RATE * FRAME_BYTES * TRACK_BUFFER_MS / 1000);
        try {
            AudioTrack track;
            if (Build.VERSION.SDK_INT >= 26) {
                track = new AudioTrack.Builder()
                        .setAudioAttributes(new AudioAttributes.Builder()
                                .setUsage(AudioAttributes.USAGE_GAME)
                                .setContentType(AudioAttributes.CONTENT_TYPE_MUSIC)
                                .build())
                        .setAudioFormat(new AudioFormat.Builder()
                                .setEncoding(AudioFormat.ENCODING_PCM_16BIT)
                                .setSampleRate(RATE)
                                .setChannelMask(mask)
                                .build())
                        .setBufferSizeInBytes(bufferBytes)
                        .setTransferMode(AudioTrack.MODE_STREAM)
                        .build();
            } else {
                track = new AudioTrack(AudioManager.STREAM_MUSIC, RATE, mask,
                        AudioFormat.ENCODING_PCM_16BIT, bufferBytes, AudioTrack.MODE_STREAM);
            }
            if (track.getState() != AudioTrack.STATE_INITIALIZED) {
                track.release();
                return null;
            }
            report("[pump] opened " + RATE + "Hz stereo buffer=" + bufferBytes
                    + "B frames=" + track.getBufferSizeInFrames()
                    + " chunk=" + chunkFrames + " lead=" + LEAD_MS + "ms");
            return track;
        } catch (RuntimeException e) {
            Log.e(TAG, "could not open AudioTrack", e);
            return null;
        }
    }

    private static void closeTrack(AudioTrack track) {
        if (track == null) {
            return;
        }
        try {
            track.stop();
        } catch (RuntimeException ignored) {
        }
        track.release();
    }

    /**
     * Logs one line to both sinks. The native one is what a collected report
     * holds; logcat is what is there when a device is attached.
     */
    private static void report(String line) {
        Log.i(TAG, line);
        try {
            NativeBridge.nativeAudioStats(line);
        } catch (Throwable ignored) {
            // The library may not be loaded yet, and a stats line is never
            // worth taking the pump down for.
        }
    }

    private static long framesToMs(long frames) {
        return frames * 1000 / RATE;
    }

    private static void sleep(long millis) {
        try {
            Thread.sleep(millis);
        } catch (InterruptedException ignored) {
        }
    }
}
