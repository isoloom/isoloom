FROM nginx:1.27-alpine
COPY index.html /usr/share/nginx/html/index.html
# /hello redirects to /hello/ (nginx does it for a folder): a check follows it.
COPY index.html /usr/share/nginx/html/hello/index.html
