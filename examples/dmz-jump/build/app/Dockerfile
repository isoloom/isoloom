# The internal app as a container: a small API on port 8080.
FROM python:3.12-alpine
COPY www/ /srv/app/
EXPOSE 8080
CMD ["python3", "-m", "http.server", "8080", "--directory", "/srv/app"]
