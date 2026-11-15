/// Reading money that the engine sends as a decimal string.
///
/// `vimbai_common::Money` serialises as a JSON *string* - `"250.000000"` - because an exact decimal does not survive a
/// double. A model field declared `double` with no converter casts that string to a number and throws, so every journal
/// entry and every trial-balance row failed to parse on the phone the moment the engine answered correctly: the screen
/// was not wrong about the arithmetic, it could not read the reply. This reads either form, so a number is still
/// accepted where one is sent.
double amountFromJson(Object? value) {
  if (value == null) return 0.0;
  if (value is num) return value.toDouble();
  if (value is String) return double.tryParse(value.trim()) ?? 0.0;
  return 0.0;
}
