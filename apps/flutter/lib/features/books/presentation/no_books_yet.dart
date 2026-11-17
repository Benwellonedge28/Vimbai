// What a person sees the first time they open Books.
//
// A stranger who has just made an account has no Books, and an empty list teaches them nothing: the only way to start was
// a small "+" in the corner with no words on the page. This is the door, in words, with the one action that matters.
//
// It is shown only when the read really came back empty. A read that failed leaves the list empty too, and answering that
// with "you have none yet" would tell someone their Books are gone when the engine was merely unreachable.
import 'package:flutter/material.dart';

class NoBooksYet extends StatelessWidget {
  const NoBooksYet({super.key, required this.onCreate});

  /// Opens the wizard that makes a Book. Called from the one button on this screen.
  final VoidCallback onCreate;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    return Padding(
      padding: const EdgeInsets.fromLTRB(24, 48, 24, 24),
      child: Column(
        mainAxisSize: MainAxisSize.min,
        children: [
          Icon(
            Icons.menu_book_outlined,
            size: 64,
            color: theme.colorScheme.primary,
          ),
          const SizedBox(height: 16),
          Text(
            'You have no Books yet',
            textAlign: TextAlign.center,
            style: theme.textTheme.titleLarge,
          ),
          const SizedBox(height: 8),
          Text(
            'A Book is where the entries live: a person\'s own, a household\'s, a group\'s or an organisation\'s. '
            'A new one arrives with a chart of accounts already set up, so you can post an entry straight away.',
            textAlign: TextAlign.center,
            style: theme.textTheme.bodyMedium,
          ),
          const SizedBox(height: 24),
          FilledButton.icon(
            onPressed: onCreate,
            icon: const Icon(Icons.add),
            label: const Text('Create your first Book'),
          ),
          const SizedBox(height: 12),
          Text(
            'Been invited to someone else\'s Book? Its invitation appears here to accept.',
            textAlign: TextAlign.center,
            style: theme.textTheme.bodySmall,
          ),
        ],
      ),
    );
  }
}
