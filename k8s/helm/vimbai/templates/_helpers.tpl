{{/* Common labels */}}
{{- define "vimbai.labels" -}}
app.kubernetes.io/name: vimbai
app.kubernetes.io/managed-by: {{ .Release.Service }}
app.kubernetes.io/instance: {{ .Release.Name }}
app.kubernetes.io/version: {{ .Chart.AppVersion }}
helm.sh/chart: {{ .Chart.Name }}-{{ .Chart.Version }}
{{- end -}}

{{/* Service name helper */}}
{{- define "vimbai.serviceName" -}}
{{- .name | replace "_" "-" | trunc 63 | trimSuffix "-" -}}
{{- end -}}

{{/* Image helper */}}
{{- define "vimbai.image" -}}
{{ .Values.global.imageRegistry }}/{{ .name }}:{{ .Values.global.imageTag | default .Chart.AppVersion }}
{{- end -}}

{{/* Resource helper */}}
{{- define "vimbai.resources" -}}
resources:
  requests:
    memory: {{ .requests.memory | default "64Mi" }}
    cpu: {{ .requests.cpu | default "50m" }}
  limits:
    memory: {{ .limits.memory | default "256Mi" }}
    cpu: {{ .limits.cpu | default "250m" }}
{{- end -}}
