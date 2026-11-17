// The signed-in person's access token, in one place.
//
// Five services - Books, personal finance, group savings, household and the non-profit screens - each held their own copy
// of a token that nothing in the app ever filled, so every request they made went out without one. A walk against a
// forgiving engine did not notice; the gateway is not forgiving, because it throws away any identity a client supplies
// and reads only the token, so a real person's Books and every screen behind them would be refused.
//
// One holder instead. AuthService fills it wherever it learns a token - a completed sign-in, a renewal, a phone restored
// at start-up - and clears it when the session ends, and each service reads it as it builds its headers, so a renewed
// token is used by every service without anything having to fan out.
class SessionToken {
  SessionToken._();

  /// The token the next request carries, or null when nobody is signed in.
  static String? value;

  static void set(String? token) {
    value = (token == null || token.isEmpty) ? null : token;
  }

  static void clear() => value = null;

  /// What a request signs itself with: a test's own token if it gave one, otherwise the session's.
  static String? or(String? own) => own ?? value;
}
