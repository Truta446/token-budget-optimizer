# Single sign-on (SSO)

SSO is available on the Enterprise plan and supports SAML 2.0 and OpenID
Connect.

## Configuring SAML
1. In *Settings → Security*, choose **Add identity provider**.
2. Upload your IdP metadata XML or paste the SSO URL and X.509 certificate.
3. Map the `email` attribute; optionally map `groups` to Acme roles.
4. Test the connection with a non-admin account before enforcing SSO.

## Enforcing SSO
Once enforced, password logins are disabled for every member except workspace
owners, who keep a break-glass password login. API keys are not affected.

## Troubleshooting SSO
- `saml_signature_invalid`: the certificate in Acme does not match the one your
  IdP signs with. Certificates usually rotate yearly; upload the new one.
- `clock_skew`: the assertion's `NotBefore` is in the future. Acme allows 3
  minutes of skew; check NTP on the IdP host.
- Users are created on first login (just-in-time provisioning). Use SCIM to
  deprovision users automatically.
