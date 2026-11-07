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
     * <p>A note reaching the mixer part way through a chunk is not rendered
     * until the next one begins, so the chunk is the grain of the whole path.
     *
     * <p>The track drains in whole HAL bursts, so writing the device's own burst
     * puts every write on a boundary its playback head can actually report.
     *
     * <p>The burst the device reports is in frames at its native output rate, so
     * it is scaled to {@link #RATE}, which is what this track runs at.
     */
    private static final int DEFAULT_CHUNK_FRAMES = 256;
    private static final int MIN_CHUNK_FRAMES = 96;
    private static final int MAX_CHUNK_FRAMES = 2048;
    /** Used until {@link #configure(Context)} reports the device's burst. */
    private static volatile int chunkFrames = DEFAULT_CHUNK_FRAMES;

    /**
     * How much audio may stand between a note and the speaker.
     *
     * <p>This is the track's own buffer, and that is the whole mechanism: a
     * blocking write returns only when the track has room, so the queue cannot
     * grow past the buffer and needs nothing to hold it there.
     *
     * <p>It used to be 180 ms with a polling gate on top that slept in 4 ms
     * steps whenever the queue read more than 60 ms, which is this job done
     * twice and done worse. A measured second of it: 252 waits, one full second
     * of sleeping inside one second, not a single blocking write - the track
     * always had room for a 3 ms chunk - and a queue swinging between 20 and 59
     * ms. A control loop whose sampling period is longer than the chunk it
     * actuates with cannot settle, and that swing is a note landing anywhere in
     * a 39 ms window depending on which part of the cycle it arrives in. The
     * gate is gone; the buffer is sized at what it was trying to hold.
     */
    private static final int TARGET_LATENCY_MS = 60;
    /**
     * Written before playback starts, so the first writes cannot drain the
     * track before the pace settles. Must stay well under the buffer: with the
     * track stopped, writes fill it and then block until {@code play()}.
     */
    private static final int PREFILL_MS = TARGET_LATENCY_MS / 2;

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
        long statsStart = System.nanoTime();
        int statPulls = 0;
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

                // The queue ahead of the playback head, which is what a note
                // starting now has to sit through. Nothing acts on it - the
                // blocking write below bounds it - but it is the number the
                // whole question turns on, so it is measured.
                if (track != null && playing) {
                    long head = track.getPlaybackHeadPosition() & 0xFFFFFFFFL;
                    long queued = Math.max(0, framesWritten - head);
                    statQueuedSamples++;
                    statQueuedSum += queued;
                    statQueuedMin = Math.min(statQueuedMin, queued);
                    statQueuedMax = Math.max(statQueuedMax, queued);
                }

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
                            + " idle=" + statIdle
                            + " underruns=+" + (underruns - lastUnderruns));
                    lastUnderruns = underruns;
                    statsStart = now;
                    statPulls = 0;
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
        int bufferBytes = Math.max(minimum, RATE * FRAME_BYTES * TARGET_LATENCY_MS / 1000);
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
                        .setPerformanceMode(AudioTrack.PERFORMANCE_MODE_LOW_LATENCY)
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
                    + " chunk=" + chunkFrames + " target=" + TARGET_LATENCY_MS + "ms");
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
