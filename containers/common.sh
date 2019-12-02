# The working directory, pre-escaped for use in a quoted CSV field.
csv_pwd=$(printf '%s' "$PWD" | sed 's/"/""/g')

# Expands a normal-looking path relative to the working directory into a src= CSV field.
expand_src() {
	printf '"src=%s/%s"' "$csv_pwd" "$1"
}

exec_docker() {
	exec docker run --rm -it \
		--runtime=runsc \
		--security-opt=no-new-privileges \
		--user="$UID:$(id -g)" \
		--read-only \
		--init \
		"$@"
}
