# Outside its build context (build/web), given as `docker.dockerfile`; the nginx tag is a build
# argument (`docker.args`).
ARG NGINX_TAG
FROM nginx:${NGINX_TAG}
COPY index.html /usr/share/nginx/html/index.html
