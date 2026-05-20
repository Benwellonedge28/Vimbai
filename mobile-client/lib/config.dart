// lib/config.dart
// This file can be replaced or updated by build scripts for different environments.

class AppConfig {
  // static const String apiUrl = 'http://localhost:8080'; // Old direct identity service URL
  static const String apiUrl = 'http://localhost:8081'; // NEW: API Gateway URL
  // All other service URLs will now be derived from this base gateway URL
  // In a real application, this would be swapped out for
  // 'https://api.finacc.dev' or 'https://api.finacc.prod'
  // using build flavors or flutter_dotenv.
}
