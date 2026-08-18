{{- define "investigator-platform.name" -}}
{{- default .Chart.Name .Values.nameOverride | trunc 63 | trimSuffix "-" -}}
{{- end -}}

{{- define "investigator-platform.appName" -}}
{{- printf "%s-%s" (include "investigator-platform.fullname" .root) .name | trunc 63 | trimSuffix "-" -}}
{{- end -}}

{{- define "investigator-platform.appServiceAccount" -}}
{{- if .app.serviceAccount.create -}}
{{- default (include "investigator-platform.appName" .) .app.serviceAccount.name | trunc 63 | trimSuffix "-" -}}
{{- else -}}
{{- required (printf "apps.%s.serviceAccount.name is required when create=false" .name) .app.serviceAccount.name | trunc 63 | trimSuffix "-" -}}
{{- end -}}
{{- end -}}

{{- define "investigator-platform.fullname" -}}
{{- if .Values.fullnameOverride -}}
{{- .Values.fullnameOverride | trunc 63 | trimSuffix "-" -}}
{{- else -}}
{{- $name := include "investigator-platform.name" . -}}
{{- if contains $name .Release.Name -}}{{ .Release.Name | trunc 63 | trimSuffix "-" }}
{{- else -}}{{ printf "%s-%s" .Release.Name $name | trunc 63 | trimSuffix "-" }}
{{- end -}}
{{- end -}}
{{- end -}}

{{- define "investigator-platform.labels" -}}
helm.sh/chart: {{ printf "%s-%s" .Chart.Name .Chart.Version | replace "+" "_" }}
app.kubernetes.io/name: {{ include "investigator-platform.name" . }}
app.kubernetes.io/instance: {{ .Release.Name }}
app.kubernetes.io/version: {{ .Chart.AppVersion | quote }}
app.kubernetes.io/managed-by: {{ .Release.Service }}
{{- end -}}

{{- define "investigator-platform.image" -}}
{{- if .image.digest -}}{{ printf "%s@%s" .image.repository .image.digest }}
{{- else -}}{{ printf "%s:%s" .image.repository (default .root.Chart.AppVersion .image.tag) }}
{{- end -}}
{{- end -}}

{{- define "investigator-platform.agentImage" -}}
{{- $repository := required "agent.image.repository is required" .Values.agent.image.repository -}}
{{- $tag := .Values.agent.image.tag | default "" -}}
{{- $digest := .Values.agent.image.digest | default "" -}}
{{- if and $tag $digest -}}{{ fail "exactly one of agent.image.tag or agent.image.digest must be set" }}{{- end -}}
{{- if not (or $tag $digest) -}}{{ fail "exactly one of agent.image.tag or agent.image.digest must be set" }}{{- end -}}
{{- if eq $tag "latest" -}}{{ fail "agent.image.tag must be an immutable version, not latest" }}{{- end -}}
{{- if $digest -}}{{ printf "%s@%s" $repository $digest }}{{- else -}}{{ printf "%s:%s" $repository $tag }}{{- end -}}
{{- end -}}

{{- define "investigator-platform.validateAgent" -}}
{{- $runtime := .Values.agent.runtime -}}
{{- $provider := .Values.agent.provider.type -}}
{{- $auth := .Values.agent.auth.type -}}
{{- if not (has $runtime (list "codex" "bedrock")) -}}{{ fail "agent.runtime must be codex or bedrock" }}{{- end -}}
{{- if not (has $provider (list "openai" "bedrock")) -}}{{ fail "agent.provider.type must be openai or bedrock" }}{{- end -}}
{{- if and (eq $runtime "bedrock") (ne $provider "bedrock") -}}{{ fail "agent.runtime=bedrock requires agent.provider.type=bedrock" }}{{- end -}}
{{- if and (eq $provider "openai") (eq $auth "workloadIdentity") -}}{{ fail "workloadIdentity authentication is only supported by Bedrock" }}{{- end -}}
{{- if and (eq $provider "bedrock") (eq $auth "authJson") -}}{{ fail "authJson authentication is only supported by OpenAI" }}{{- end -}}
{{- if eq $provider "bedrock" -}}
{{- $region := required "agent.provider.region is required for Bedrock" .Values.agent.provider.region -}}
{{- $projectId := required "agent.provider.projectId is required for Bedrock" .Values.agent.provider.projectId -}}
{{- if eq $runtime "bedrock" -}}
{{- if not (has .Values.agent.provider.api (list "responses" "chatCompletions")) -}}{{ fail "agent.provider.api must be responses or chatCompletions for the bedrock runtime" }}{{- end -}}
{{- else if .Values.agent.provider.api -}}{{ fail "agent.provider.api must be empty for the codex runtime" }}
{{- else if not (hasPrefix "openai." .Values.agent.provider.model) -}}{{ fail "the codex runtime requires a Bedrock OpenAI model ID beginning with openai." }}{{- end -}}
{{- else if .Values.agent.provider.api -}}{{ fail "agent.provider.api is only supported by the bedrock runtime" }}{{- end -}}
{{- $image := include "investigator-platform.agentImage" . -}}
{{- end -}}

{{- define "investigator-platform.controllerServiceAccount" -}}
{{- if .Values.investigator.serviceAccount.create -}}
{{- default (printf "%s-controller" (include "investigator-platform.fullname" .)) .Values.investigator.serviceAccount.name | trunc 63 | trimSuffix "-" -}}
{{- else -}}
{{- required "investigator.serviceAccount.name is required when create=false" .Values.investigator.serviceAccount.name | trunc 63 | trimSuffix "-" -}}
{{- end -}}
{{- end -}}

{{- define "investigator-platform.agentServiceAccount" -}}
{{- if .Values.agent.serviceAccount.create -}}
{{- default (printf "%s-agent" (include "investigator-platform.fullname" .)) .Values.agent.serviceAccount.name | trunc 63 | trimSuffix "-" -}}
{{- else -}}
{{- required "agent.serviceAccount.name is required when create=false" .Values.agent.serviceAccount.name | trunc 63 | trimSuffix "-" -}}
{{- end -}}
{{- end -}}

{{- define "investigator-platform.serverName" -}}
{{- printf "%s-%s" (include "investigator-platform.fullname" .root) .name | trunc 63 | trimSuffix "-" -}}
{{- end -}}

{{- define "investigator-platform.serverServiceAccount" -}}
{{- if .server.serviceAccount.create -}}
{{- default (include "investigator-platform.serverName" .) .server.serviceAccount.name | trunc 63 | trimSuffix "-" -}}
{{- else -}}
{{- required (printf "mcpServers.%s.serviceAccount.name is required when create=false" .name) .server.serviceAccount.name | trunc 63 | trimSuffix "-" -}}
{{- end -}}
{{- end -}}
