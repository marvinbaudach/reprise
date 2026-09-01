function close_run() {
  if (output == "") {
    return
  }
  if (style == "folded") {
    printf "\n" >> output
  }
  close(output)
  output = ""
  style = ""
  content_indent = 0
}

function open_run(value) {
  output = sprintf("%s/%s.run-%d.sh", output_dir, workflow_name, FNR)
  print "#!/usr/bin/env bash" > output
  if (value ~ /^\|[-+]?$/) {
    style = "literal"
  } else if (value ~ /^>[-+]?$/) {
    style = "folded"
  } else {
    print value >> output
    close_run()
  }
}

function maybe_open_run(line, leading, value) {
  if (line !~ /^ +run:[ ]*/) {
    return
  }
  leading = line
  sub(/[^ ].*$/, "", leading)
  run_indent = length(leading)
  value = line
  sub(/^ +run:[ ]*/, "", value)
  open_run(value)
}

{
  line = $0
  if (output != "") {
    if (line ~ /^ *$/) {
      print "" >> output
      next
    }
    leading = line
    sub(/[^ ].*$/, "", leading)
    if (content_indent == 0 && length(leading) > run_indent) {
      content_indent = length(leading)
    }
    if (content_indent > 0 && length(leading) >= content_indent) {
      content = substr(line, content_indent + 1)
      if (style == "folded") {
        printf "%s ", content >> output
      } else {
        print content >> output
      }
      next
    }
    close_run()
  }
  maybe_open_run(line)
}

END {
  close_run()
}
