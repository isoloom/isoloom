# The invoice portal API as a container: Python's own HTTP server and one database driver, on 80.
FROM python:3.12-slim
RUN pip install --no-cache-dir PyMySQL==1.1.1
COPY portal.py /app/portal.py
ENV DB_HOST=database DB_PORT=3207 DB_USER=portal DB_PASSWORD=example-portal-password DB_NAME=portal
EXPOSE 80
CMD ["python3", "/app/portal.py"]
