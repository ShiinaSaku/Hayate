#!/usr/bin/env bash
set -e

REPO="ShiinaSaku/Hayate"
OUTPUT_FILE="CHANGELOG.md"

generate_section() {
    local version="$1"
    local range="$2"
    local date_ref

    # Resolve date of the section
    if [ "$range" = "$version" ]; then
        date_ref="$version"
    else
        date_ref="${range#*..}"
    fi
    local date_val
    date_val=$(git log -1 --format=%cd --date=short "$date_ref" 2>/dev/null || date +%Y-%m-%d)

    # Fetch commits
    local commits
    if [ "$range" = "$version" ]; then
        commits=$(git log --oneline "$version")
    else
        commits=$(git log --oneline "$range")
    fi

    if [ -z "$commits" ]; then
        return
    fi

    echo "## [$version] - $date_val" >> "$OUTPUT_FILE"
    echo "" >> "$OUTPUT_FILE"

    local features=""
    local fixes=""
    local refactors=""
    local chores=""
    local docs=""
    local other=""

    while IFS= read -r line; do
        if [ -z "$line" ]; then continue; fi

        local hash
        hash=$(echo "$line" | awk '{print $1}')
        local message
        message=$(echo "$line" | cut -d' ' -f2-)

        # Format commit link
        local formatted_line="- $message ([\`$hash\`](https://github.com/${REPO}/commit/${hash}))\n"

        # Classification check (case-insensitive)
        local lower_message
        lower_message=$(echo "$message" | tr '[:upper:]' '[:lower:]')

        if [[ $lower_message =~ ^feat(\(.*\))?: ]]; then
            features="${features}${formatted_line}"
        elif [[ $lower_message =~ ^fix(\(.*\))?: ]]; then
            fixes="${fixes}${formatted_line}"
        elif [[ $lower_message =~ ^refactor(\(.*\))?: ]] || [[ $lower_message =~ ^style(\(.*\))?: ]]; then
            refactors="${refactors}${formatted_line}"
        elif [[ $lower_message =~ ^docs(\(.*\))?: ]]; then
            docs="${docs}${formatted_line}"
        elif [[ $lower_message =~ ^chore(\(.*\))?: ]] || [[ $lower_message =~ ^build(\(.*\))?: ]] || [[ $lower_message =~ ^ci(\(.*\))?: ]]; then
            chores="${chores}${formatted_line}"
        else
            other="${other}${formatted_line}"
        fi
    done <<< "$commits"

    if [ -n "$features" ]; then
        echo "### ✦ Features" >> "$OUTPUT_FILE"
        printf "%b" "$features" >> "$OUTPUT_FILE"
        echo "" >> "$OUTPUT_FILE"
    fi
    if [ -n "$fixes" ]; then
        echo "### ✦ Bug Fixes" >> "$OUTPUT_FILE"
        printf "%b" "$fixes" >> "$OUTPUT_FILE"
        echo "" >> "$OUTPUT_FILE"
    fi
    if [ -n "$refactors" ]; then
        echo "### ✦ Refactoring & Code Quality" >> "$OUTPUT_FILE"
        printf "%b" "$refactors" >> "$OUTPUT_FILE"
        echo "" >> "$OUTPUT_FILE"
    fi
    if [ -n "$docs" ]; then
        echo "### ✦ Documentation" >> "$OUTPUT_FILE"
        printf "%b" "$docs" >> "$OUTPUT_FILE"
        echo "" >> "$OUTPUT_FILE"
    fi
    if [ -n "$chores" ]; then
        echo "### ✦ Maintenance & Chore" >> "$OUTPUT_FILE"
        printf "%b" "$chores" >> "$OUTPUT_FILE"
        echo "" >> "$OUTPUT_FILE"
    fi
    if [ -n "$other" ]; then
        echo "### ✦ Miscellaneous" >> "$OUTPUT_FILE"
        printf "%b" "$other" >> "$OUTPUT_FILE"
        echo "" >> "$OUTPUT_FILE"
    fi

    echo "" >> "$OUTPUT_FILE"
}

# Initialize file
echo "# Changelog" > "$OUTPUT_FILE"
echo "" >> "$OUTPUT_FILE"
echo "All notable changes to this project will be documented in this file." >> "$OUTPUT_FILE"
echo "" >> "$OUTPUT_FILE"

# Get all tags in reverse semantic order
tags=($(git tag --sort=-v:refname))

# Unreleased/Development since the last tag (which we label as v2.0.0)
last_tag=${tags[0]}
if [ -n "$last_tag" ]; then
    generate_section "v2.0.0" "$last_tag..HEAD"
fi

# Historical tag ranges
for ((i=0; i<${#tags[@]}; i++)); do
    current_tag=${tags[i]}
    next_tag=${tags[i+1]}

    if [ -n "$next_tag" ]; then
        generate_section "$current_tag" "$next_tag..$current_tag"
    else
        generate_section "$current_tag" "$current_tag"
    fi
done

echo "Changelog generated successfully in ${OUTPUT_FILE}!"
