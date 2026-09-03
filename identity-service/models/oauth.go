package models

import "time"

// OAuthProvider holds the configuration for a single OAuth2/OIDC provider
// (client credentials, endpoint URLs and requested scopes).
type OAuthProvider struct {
	Provider     string   `json:"provider"`
	ClientID     string   `json:"client_id"`
	ClientSecret string   `json:"-"`
	AuthURL      string   `json:"auth_url"`
	TokenURL     string   `json:"token_url"`
	UserInfoURL  string   `json:"user_info_url"`
	Scopes       []string `json:"scopes"`
	Enabled      bool     `json:"enabled"`
}

// OAuthProviderInfo is the public (non-sensitive) description of an enabled
// OAuth provider, as returned by the GET /auth/oauth/providers endpoint.
type OAuthProviderInfo struct {
	Name        string `json:"name"`
	DisplayName string `json:"display_name"`
	Icon        string `json:"icon"`
}

// OAuthTokenResponse is the token payload returned by the provider's
// token endpoint during the authorization_code exchange.
type OAuthTokenResponse struct {
	AccessToken  string `json:"access_token"`
	TokenType    string `json:"token_type"`
	RefreshToken string `json:"refresh_token,omitempty"`
	ExpiresIn    int    `json:"expires_in,omitempty"`
	Scope        string `json:"scope,omitempty"`
}

// OAuthUserInfo is the normalized user profile fetched from the
// provider's user info endpoint.
type OAuthUserInfo struct {
	ProviderID string `json:"provider_id"`
	Email      string `json:"email,omitempty"`
	Name       string `json:"name,omitempty"`
	Username   string `json:"username,omitempty"`
	Picture    string `json:"picture,omitempty"`
}

// OAuthSession is the short-lived CSRF state session created when
// redirecting a user to an OAuth provider's authorize endpoint.
type OAuthSession struct {
	Provider  string    `json:"provider"`
	CreatedAt time.Time `json:"created_at"`
}

// OIDCDiscovery is the OpenID Connect discovery document served at
// /.well-known/openid-configuration.
type OIDCDiscovery struct {
	Issuer                            string   `json:"issuer"`
	AuthorizationEndpoint             string   `json:"authorization_endpoint"`
	TokenEndpoint                     string   `json:"token_endpoint"`
	UserInfoEndpoint                  string   `json:"userinfo_endpoint"`
	JwksURI                           string   `json:"jwks_uri"`
	ResponseTypesSupported            []string `json:"response_types_supported"`
	SubjectTypesSupported             []string `json:"subject_types_supported"`
	IDTokenSigningAlgValuesSupported  []string `json:"id_token_signing_alg_values_supported"`
	ScopesSupported                   []string `json:"scopes_supported"`
	TokenEndpointAuthMethodsSupported []string `json:"token_endpoint_auth_methods_supported"`
	ClaimsSupported                   []string `json:"claims_supported"`
}

// JWK is a single JSON Web Key entry in the published key set.
type JWK struct {
	Kty string `json:"kty"`
	Use string `json:"use"`
	Kid string `json:"kid"`
	Alg string `json:"alg"`
	N   string `json:"n"`
	E   string `json:"e"`
}

// JWKS is the JSON Web Key Set document served at /.well-known/jwks.json.
type JWKS struct {
	Keys []JWK `json:"keys"`
}
