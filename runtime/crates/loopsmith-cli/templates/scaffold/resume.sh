{{header}}
if [ $# -eq 0 ]; then
  echo "usage: ./resume.sh <run-id>" >&2
  echo "The run id is printed at the end of every run and names the file in logs/." >&2
  echo "recent runs:" >&2
  # `ls -1t` and `sed` behave the same on either userland here. The flag that
  # differs between GNU and BSD is `-i`, which this does not use.
  ls -1t logs/ 2>/dev/null | head -5 | sed -e 's/\.log$//' -e 's/^/  /' >&2
  exit 2
fi
exec "$LOOPSMITH" resume "{{config_file}}" "$1"
