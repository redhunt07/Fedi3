#!/usr/bin/env bash
# Read deployment values as data. Never eval/source a deployment's .env.
# Supports literal KEY=VALUE, optional export and matching outer quotes.
load_dotenv() {
  local dotenv_file="$1" dotenv_line dotenv_key dotenv_value
  [[ -f "$dotenv_file" ]] || return 0
  while IFS= read -r dotenv_line || [[ -n "$dotenv_line" ]]; do
    dotenv_line="${dotenv_line%$'\r'}"
    [[ "$dotenv_line" =~ ^[[:space:]]*(#|$) ]] && continue
    if [[ ! "$dotenv_line" =~ ^[[:space:]]*(export[[:space:]]+)?([A-Za-z_][A-Za-z0-9_]*)[[:space:]]*=(.*)$ ]]; then
      echo "Invalid dotenv assignment in $dotenv_file" >&2
      return 1
    fi
    dotenv_key="${BASH_REMATCH[2]}"
    dotenv_value="${BASH_REMATCH[3]}"
    dotenv_value="${dotenv_value#"${dotenv_value%%[![:space:]]*}"}"
    dotenv_value="${dotenv_value%"${dotenv_value##*[![:space:]]}"}"
    if [[ "$dotenv_value" == \"*\" || "$dotenv_value" == \'*\' ]]; then
      dotenv_value="${dotenv_value:1:${#dotenv_value}-2}"
    else
      # Compose-style inline comments require whitespace before '#'.
      dotenv_value="${dotenv_value%%[[:space:]]#*}"
    fi
    # Do not overwrite an explicit environment override, or shell internals.
    case "$dotenv_key" in
      FEDI3_*|TURN_*|MEILI_*|BACKUP_*)
        if [[ ! -v "$dotenv_key" ]]; then
          # Keep values in this shell. Compose reads/interpolates its own .env;
          # exporting literal ${VAR} strings would override that resolution.
          printf -v "$dotenv_key" '%s' "$dotenv_value"
        fi
        ;;
    esac
  done < "$dotenv_file"
}
