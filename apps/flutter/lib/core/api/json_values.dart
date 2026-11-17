/// Reading a flag that can arrive as the store keeps it.
///
/// SQLite has no boolean: the local tables write `is_synced` as `1` or `0`, while the engine writes `true` or `false`.
/// A model field declared `bool?` with a plain cast therefore throws on every row the phone itself wrote - which is how
/// the offline queue became unreadable: the entries waiting to be sent were there, and the read threw before it could
/// list them.
bool? flagFromJson(Object? value) {
  if (value == null) return null;
  if (value is bool) return value;
  if (value is num) return value != 0;
  if (value is String) {
    final text = value.trim().toLowerCase();
    if (text == 'true' || text == '1') return true;
    if (text == 'false' || text == '0') return false;
  }
  return null;
}
