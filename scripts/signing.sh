#!/bin/bash
# A local signing identity that stays the same between builds.
#
# With ad-hoc signing the app's identity is the hash of the build: after every rebuild macOS sees
# a new app and the Accessibility permission no longer applies (although it still looks enabled).
# With a self-signed certificate the identity is "bundle id + this certificate", so the permission
# survives rebuilds.
#
# The certificate lives in a separate keychain used only by this project (not the login keychain,
# not on the system search list). It is not trusted for anything else and is not for distribution:
# other Macs will still show the Gatekeeper warning.
#
#   source scripts/signing.sh && vf_ensure_identity   # sets VF_KEYCHAIN and VF_IDENTITY

VF_SIGN_DIR="$HOME/Library/Application Support/VoiceFlow/signing"
VF_KEYCHAIN="$VF_SIGN_DIR/voiceflow-signing.keychain-db"
VF_CERT_NAME="Voice Flow Local Signing"

vf_ensure_identity() {
    mkdir -p "$VF_SIGN_DIR"
    chmod 700 "$VF_SIGN_DIR"
    local pass_file="$VF_SIGN_DIR/keychain-password"

    if [[ ! -f "$VF_KEYCHAIN" || ! -f "$pass_file" ]]; then
        echo "   Creating the local signing certificate (once)..."
        rm -f "$VF_KEYCHAIN" "$pass_file"
        local tmp
        tmp="$(mktemp -d)"
        (umask 077 && openssl rand -hex 24 > "$pass_file")
        local pass
        pass="$(cat "$pass_file")"

        cat > "$tmp/cert.cnf" <<CNF
[req]
distinguished_name = dn
x509_extensions = ext
prompt = no
[dn]
CN = $VF_CERT_NAME
[ext]
basicConstraints = critical,CA:FALSE
keyUsage = critical,digitalSignature
extendedKeyUsage = critical,codeSigning
CNF
        openssl req -x509 -newkey rsa:2048 -nodes -days 3650 -config "$tmp/cert.cnf" \
            -keyout "$tmp/key.pem" -out "$tmp/cert.pem" 2>/dev/null
        # -legacy: `security import` cannot read PKCS#12 written with OpenSSL 3's default algorithms.
        openssl pkcs12 -export -legacy -inkey "$tmp/key.pem" -in "$tmp/cert.pem" \
            -name "$VF_CERT_NAME" -out "$tmp/id.p12" -passout "pass:$pass" 2>/dev/null \
            || openssl pkcs12 -export -inkey "$tmp/key.pem" -in "$tmp/cert.pem" \
                -name "$VF_CERT_NAME" -out "$tmp/id.p12" -passout "pass:$pass"

        security create-keychain -p "$pass" "$VF_KEYCHAIN"
        security set-keychain-settings "$VF_KEYCHAIN" # no auto-lock
        security unlock-keychain -p "$pass" "$VF_KEYCHAIN"
        security import "$tmp/id.p12" -k "$VF_KEYCHAIN" -P "$pass" -T /usr/bin/codesign >/dev/null
        security set-key-partition-list -S apple-tool:,apple: -s -k "$pass" "$VF_KEYCHAIN" >/dev/null
        rm -rf "$tmp"
    fi

    security unlock-keychain -p "$(cat "$pass_file")" "$VF_KEYCHAIN"
    # The certificate's SHA-1: codesign accepts it as an identity even though the certificate is not trusted.
    VF_IDENTITY="$(security find-certificate -c "$VF_CERT_NAME" -Z "$VF_KEYCHAIN" | awk '/SHA-1/ {print $NF}')"
    [[ -n "$VF_IDENTITY" ]] || { echo "Cannot find the certificate in $VF_KEYCHAIN" >&2; return 1; }
    export VF_KEYCHAIN VF_IDENTITY
}
