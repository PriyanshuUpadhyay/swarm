#!/bin/sh
test "$*" = 'app-server --listen stdio://' || exit 8
while IFS= read -r request; do
  /bin/echo "$CODEX_HOME|$request" >> "$HOME/requests"
  case "$request" in
    *'"method":"initialize"'*) echo '{"id":1,"result":{"userAgent":"fixture"}}' ;;
    *'"method":"initialized"'*) ;;
    *'"method":"account/read"'*'"refreshToken":false'*)
      case "$CODEX_HOME" in
        */.codex-personal) echo '{"id":2,"result":{"account":{"type":"apiKey"}}}' ;;
        */.codex-work) echo '{"id":2,"result":{"account":{"type":"chatgpt","email":"owner@example.test"}}}' ;;
        *) echo '{"id":2,"result":{"account":null}}' ;;
      esac ;;
    *'"method":"account/rateLimits/read"'*)
      /bin/echo '{"method":"account/rateLimits/updated","params":{}}'
      /bin/cat "$HOME/limits.json"
      echo ;;
    *) exit 9 ;;
  esac
done
