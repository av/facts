#!/usr/bin/env bash
set -euo pipefail

SKILLS_DIR="$(cd "$(dirname "$0")/skills" && pwd)"

usage() {
    cat <<'EOF'
Usage: skills.sh <command> [args]

Commands:
    list                    List available skills
    show <skill>            Print a skill's content
    install <skill> <dest>  Copy a skill file to a destination directory
    path <skill>            Print the absolute path to a skill file

Skills:
    facts              Teaches agents how to install and use the facts CLI
    facts-discover     Instructs agent to scan codebase and maintain fact sheet
    facts-implement    Instructs agent to implement facts as code

EOF
}

list_skills() {
    for dir in "$SKILLS_DIR"/*/; do
        name="$(basename "$dir")"
        if [ -f "$dir/SKILL.md" ]; then
            head -1 "$dir/SKILL.md" | sed 's/^# //'
            printf "    %s (%s/SKILL.md)\n\n" "$name" "$dir"
        fi
    done
}

show_skill() {
    local name="$1"
    local file="$SKILLS_DIR/$name/SKILL.md"
    if [ ! -f "$file" ]; then
        echo "error: unknown skill '$name'" >&2
        echo "run 'skills.sh list' to see available skills" >&2
        exit 1
    fi
    cat "$file"
}

install_skill() {
    local name="$1"
    local dest="$2"
    local file="$SKILLS_DIR/$name/SKILL.md"
    if [ ! -f "$file" ]; then
        echo "error: unknown skill '$name'" >&2
        exit 1
    fi
    mkdir -p "$dest"
    cp "$file" "$dest/SKILL.md"
    echo "installed $name to $dest/SKILL.md"
}

path_skill() {
    local name="$1"
    local file="$SKILLS_DIR/$name/SKILL.md"
    if [ ! -f "$file" ]; then
        echo "error: unknown skill '$name'" >&2
        exit 1
    fi
    echo "$file"
}

if [ $# -eq 0 ]; then
    usage
    exit 0
fi

case "$1" in
    list)
        list_skills
        ;;
    show)
        [ $# -lt 2 ] && { echo "error: 'show' requires a skill name" >&2; exit 1; }
        show_skill "$2"
        ;;
    install)
        [ $# -lt 3 ] && { echo "error: 'install' requires a skill name and destination" >&2; exit 1; }
        install_skill "$2" "$3"
        ;;
    path)
        [ $# -lt 2 ] && { echo "error: 'path' requires a skill name" >&2; exit 1; }
        path_skill "$2"
        ;;
    -h|--help|help)
        usage
        ;;
    *)
        echo "error: unknown command '$1'" >&2
        usage
        exit 1
        ;;
esac
