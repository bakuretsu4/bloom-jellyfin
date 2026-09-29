// Spike: does a Flutter Linux window run at the monitor's refresh rate, and does it do so with
// mpv (media_kit) video playing under Material 3 controls? Nothing here is Bloom code.
import 'dart:io';

import 'package:flutter/material.dart';
import 'package:flutter/scheduler.dart';
import 'package:media_kit/media_kit.dart';
import 'package:media_kit_video/media_kit_video.dart';

// A video to play: `VIDEO=/path/to/file.mkv flutter run ...`, else a public sample.
const _sample = 'https://download.blender.org/peach/bigbuckbunny_movies/big_buck_bunny_1080p_h264.mov';

void main() {
  WidgetsFlutterBinding.ensureInitialized();
  MediaKit.ensureInitialized();
  runApp(const SpikeApp());
}

class SpikeApp extends StatelessWidget {
  const SpikeApp({super.key});

  @override
  Widget build(BuildContext context) => MaterialApp(
        title: 'Flutter fps spike',
        debugShowCheckedModeBanner: false,
        theme: ThemeData(
          useMaterial3: true,
          colorScheme: ColorScheme.fromSeed(seedColor: const Color(0xFFE0862B), brightness: Brightness.dark),
        ),
        home: const SpikePage(),
      );
}

class SpikePage extends StatefulWidget {
  const SpikePage({super.key});

  @override
  State<SpikePage> createState() => _SpikePageState();
}

class _SpikePageState extends State<SpikePage> with SingleTickerProviderStateMixin {
  late final Player player = Player();
  late final VideoController controller = VideoController(player);
  late final AnimationController spin = AnimationController(vsync: this, duration: const Duration(seconds: 2))..repeat();

  final ValueNotifier<String> readout = ValueNotifier('measuring...');
  int frames = 0;
  double worstMs = 0;
  Duration? lastFrame;
  Duration windowStart = Duration.zero;
  bool showVideo = true;

  @override
  void initState() {
    super.initState();
    player.open(Media(Platform.environment['VIDEO'] ?? _sample));
    // Every frame the engine actually produces; a persistent callback runs once per frame.
    SchedulerBinding.instance.addPersistentFrameCallback((Duration now) {
      frames++;
      if (lastFrame != null) {
        final ms = (now - lastFrame!).inMicroseconds / 1000;
        if (ms > worstMs) worstMs = ms;
      }
      lastFrame = now;
      if (windowStart == Duration.zero) windowStart = now;
      final elapsed = now - windowStart;
      if (elapsed >= const Duration(seconds: 1)) {
        final fps = frames * 1e6 / elapsed.inMicroseconds;
        readout.value = '${fps.round()} fps   worst ${worstMs.toStringAsFixed(1)} ms';
        frames = 0;
        worstMs = 0;
        windowStart = now;
      }
    });
  }

  @override
  void dispose() {
    spin.dispose();
    player.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final scheme = Theme.of(context).colorScheme;
    return Scaffold(
      body: Stack(
        children: [
          // Layer 1: video from mpv (or nothing, to compare).
          if (showVideo) Positioned.fill(child: Video(controller: controller, controls: NoVideoControls)),
          // Layer 2: Material 3 things over the picture, animating: a spinner, a fast-moving bar,
          // and a horizontally scrolling row of cards. Any of these dropping to 60 shows in the readout.
          Positioned(
            left: 24,
            right: 24,
            bottom: 96,
            height: 140,
            child: ListView.separated(
              scrollDirection: Axis.horizontal,
              itemCount: 40,
              separatorBuilder: (_, __) => const SizedBox(width: 12),
              itemBuilder: (_, i) => Card(
                elevation: 3,
                color: scheme.surfaceContainerHigh.withOpacity(0.92),
                child: SizedBox(width: 200, child: Center(child: Text('Card $i'))),
              ),
            ),
          ),
          Positioned(
            left: 24,
            right: 24,
            bottom: 40,
            child: AnimatedBuilder(
              animation: spin,
              builder: (_, __) => LinearProgressIndicator(value: spin.value, minHeight: 8, borderRadius: BorderRadius.circular(4)),
            ),
          ),
          Positioned(
            top: 24,
            left: 24,
            child: Row(children: [
              RotationTransition(turns: spin, child: Icon(Icons.autorenew, size: 48, color: scheme.primary)),
              const SizedBox(width: 16),
              FilledButton.tonal(onPressed: () => player.playOrPause(), child: const Text('Play / pause')),
              const SizedBox(width: 8),
              FilledButton.tonal(onPressed: () => setState(() => showVideo = !showVideo), child: Text(showVideo ? 'Hide video' : 'Show video')),
            ]),
          ),
          // Layer 3: the readout.
          Positioned(
            top: 24,
            right: 24,
            child: ValueListenableBuilder<String>(
              valueListenable: readout,
              builder: (_, text, __) => Container(
                padding: const EdgeInsets.symmetric(horizontal: 12, vertical: 8),
                decoration: BoxDecoration(color: Colors.black87, borderRadius: BorderRadius.circular(8)),
                child: Text(text, style: const TextStyle(fontFamily: 'monospace', fontSize: 18, color: Colors.white)),
              ),
            ),
          ),
        ],
      ),
    );
  }
}
