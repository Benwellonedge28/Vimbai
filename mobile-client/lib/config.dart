// lib/config.dart
// Environment-based configuration for Vimbai mobile client.
//
// The API URL is set at build time with --dart-define, so the same binary
// works against any backend:
//
//   flutter run --dart-define=VIMBAI_API_URL=http://192.168.1.50:8081
//   flutter build apk --dart-define=VIMBAI_API_URL=https://api.vimbai.example
//
// If not provided, it falls back to the 10.0.2.2 loopback (Android
// emulator -> host machine) so local development works out of the box.

class AppConfig {
  // Gateway base URL. All service calls route through the API gateway.
  static const String apiUrl = String.fromEnvironment(
    'VIMBAI_API_URL',
    defaultValue: 'http://10.0.2.2:8081', // Android emulator -> host machine
  );

  // Gateway route prefixes for each backend service.
  static const String identityRoute = '$apiUrl/identity';
  static const String accountingRoute = '$apiUrl/accounting';
  static const String bankingRoute = '$apiUrl/banking-integration';
  static const String budgetsRoute = '$apiUrl/budget';
  static const String multimodalRoute = '$apiUrl/multimodal';

  // Network behaviour tuned for offline-first usage:
  // short timeouts fail fast so the UI can fall back to local data
  // instead of hanging.
  static const Duration connectTimeout = Duration(seconds: 5);
  static const Duration requestTimeout = Duration(seconds: 15);
  static const int maxRetries = 2;

  // Gateway route for the cash flow statement service.
  static const String cashFlowStatementRoute = '$apiUrl/cash-flow-statement';
}
