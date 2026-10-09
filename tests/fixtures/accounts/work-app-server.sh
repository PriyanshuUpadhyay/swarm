#!/bin/sh
test "$*" = 'app-server --listen stdio://' || exit 8
IFS= read -r request
case "$request" in *'"method":"initialize"'*) ;; *) exit 9 ;; esac
echo '{"id":1,"result":{"userAgent":"fixture"}}'
IFS= read -r request
case "$request" in *'"method":"initialized"'*) ;; *) exit 10 ;; esac
IFS= read -r request
case "$request" in *'"method":"account/read"'*'"refreshToken":false'*) ;; *) exit 11 ;; esac
case "$CODEX_HOME" in
  */.codex-personal) echo '{"id":2,"result":{"account":null,"requiresOpenaiAuth":true}}' ;;
  *) echo '{"method":"account/updated","params":{"authMode":"chatgpt"}}'
     echo '{"id":99,"result":{}}'
     echo '{"id":2,"result":{"account":{"type":"chatgpt","email":"owner@example.test","planType":"plus"},"requiresOpenaiAuth":true}}' ;;
esac
while IFS= read -r request; do :; done
