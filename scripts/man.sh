#!/bin/sh
set -eu

if [ "$#" -ne 1 ] || [ -z "$1" ]; then
    printf 'Usage: ./scripts/man.sh BINARY\nGenerates git-ls.1 from the supplied git-ls executable.\n' >&2
    exit 2
fi

# Capture help before formatting so an executable failure cannot be masked by awk.
help_text=$(NO_COLOR=1 CLICOLOR_FORCE=0 "$1" --help)
printf '%s\n' "$help_text" | awk '
    function literal(line, output, i, character) {
        output = "\\&"
        for (i = 1; i <= length(line); i++) {
            character = substr(line, i, 1)
            if (character == "\\") output = output "\\e"
            else if (character == "-") output = output "\\-"
            else output = output character
        }
        return output
    }
    BEGIN { print ".TH GIT-LS 1\n.SH NAME" }
    NR == 1 {
        print "git\\-ls \\- " literal($0)
        next
    }
    /^Usage: / {
        print ".SH SYNOPSIS\n.nf"
        print literal(substr($0, 8))
        print ".fi"
        next
    }
    /^(Arguments|Options):$/ {
        section = $0
        sub(/:$/, "", section)
        print ".SH " toupper(section)
        next
    }
    /^  [^ ]|^      -/ {
        line = $0
        sub(/^[ \t]+/, "", line)
        print ".TP\n.B"
        if (match(line, /  +/)) {
            heading = substr(line, 1, RSTART - 1)
            description = substr(line, RSTART + RLENGTH)
            print literal(heading)
            if (description != "") print literal(description)
        } else {
            print literal(line)
        }
        next
    }
    NF {
        sub(/^[ \t]+/, "")
        print literal($0)
    }
    END { print ".SH SEE ALSO\n.BR git (1)" }
'
