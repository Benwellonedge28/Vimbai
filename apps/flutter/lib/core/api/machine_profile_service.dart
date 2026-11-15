// Asking the machine what it serves, once, when the app opens.

import 'dart:convert';

import '../../app/config.dart';
import 'api_client.dart';
import 'machine_profile.dart';

/// Reads `/health` and hands back the machine's own account of itself.
///
/// The health door is open on every profile - it is the one address a caller can reach before signing in - so this needs no
/// token and cannot fail for a reason the app would have to explain. A machine that is not there, or an answer that is not
/// JSON, means the app assumes it serves everything and lets each screen find out for itself.
class MachineProfileService {
  const MachineProfileService();

  Future<MachineProfile> read() async {
    try {
      final response = await ApiClient().get(
        Uri.parse('${AppConfig.apiUrl}/health'),
      );
      if (response.statusCode != 200) {
        return MachineProfile.everything;
      }
      final body = jsonDecode(response.body);
      return MachineProfile.fromHealth(body);
    } catch (_) {
      return MachineProfile.everything;
    }
  }
}
