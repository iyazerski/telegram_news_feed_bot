# Builds every service image from the one Dockerfile, whose shared build stage runs once for all of them.
# CD sets both variables from the environment; locally, `docker buildx bake` builds images tagged `<service>-local`.

variable "IMAGE_REPOSITORY" {
  default = "telegram-news-feed-bot"
}

variable "IMAGE_TAG" {
  default = "local"
}

group "default" {
  targets = ["bot", "feed", "migration"]
}

target "service" {
  name       = service
  matrix     = { service = ["bot", "feed", "migration"] }
  context    = "."
  dockerfile = "docker/Dockerfile"
  target     = service
  tags       = ["${IMAGE_REPOSITORY}:${service}-${IMAGE_TAG}", "${IMAGE_REPOSITORY}:${service}-latest"]
}
