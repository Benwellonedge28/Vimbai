import 'package:flutter/material.dart';

/// What a screen says when the engine could not answer.
///
/// A raw exception printed on a page tells a person nothing they can act on, and an empty list shown instead of a
/// failed read tells them their work is gone when it is not. This names what could not be read, repeats the reason in
/// plain words, and offers the one useful action.
class CouldNotRead extends StatelessWidget {
  const CouldNotRead({super.key, required this.what, this.error, this.onRetry});

  /// The thing that could not be read, as it is named to the person: "The trial balance".
  final String what;

  /// What came back instead, so its reason can be passed on.
  final Object? error;

  /// Offered only where asking again could help.
  final VoidCallback? onRetry;

  /// The engine's own words, with the scaffold of an exception taken off the front so a person reads a sentence.
  static String reasonOf(Object? error) =>
      (error ?? 'no reason given').toString().replaceFirst('Exception: ', '');

  @override
  Widget build(BuildContext context) {
    return Center(
      child: Padding(
        padding: const EdgeInsets.all(24.0),
        child: Column(
          mainAxisAlignment: MainAxisAlignment.center,
          children: [
            const Icon(Icons.cloud_off, size: 48),
            const SizedBox(height: 12),
            Text(
              '$what could not be read just now.',
              style: const TextStyle(fontWeight: FontWeight.bold),
              textAlign: TextAlign.center,
            ),
            const SizedBox(height: 6),
            Text(reasonOf(error), textAlign: TextAlign.center),
            if (onRetry != null) ...[
              const SizedBox(height: 16),
              OutlinedButton(
                onPressed: onRetry,
                child: const Text('Try again'),
              ),
            ],
          ],
        ),
      ),
    );
  }
}
