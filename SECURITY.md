# Security Policy

shimbox is maintained by one person. There is no service-level agreement here and
no bug bounty: what there is is an answer, and the rule below on how to reach
the code.

## Reporting a vulnerability

Please report it privately, through
[GitHub Security Advisories](https://github.com/goloveshko/shimbox/security/advisories/new),
rather than a public issue, so nobody has to run an unfixed build while the report
is open. Either language is fine.

A report that says how to reproduce it is a report that gets fixed. A affected
version and a patch or a description of one is welcome but never required.

You will get an answer within about a week. The fix ships in the next release,
and the advisory publishes as soon as that release is out, so the people running
the old build learn they should move.

## What the code holds

- **Secrets are not in the config.** A `.shim` file carries an `envref` naming a
  Windows Credential Manager entry, never the value. The value leaves the store
  only in the environment block of the process the shim launches.
- **A shim pair is written once and never edited silently.** An existing
  `.exe` is overwritten only when its version resource names this project as the
  product and one of the two runner builds as the original file (docs/techspec.md
  5.7) - two names, because the manager carries the first of them too; generating
  over a name already in the destination folder is refused.
- **The runner reads one config named after itself**, from beside its own file,
  and nothing else.

So the boundary worth reporting around is a way to have a runner launch something
it was not told to launch, a secret read by a process other than its own target,
or a path the manager shows or opens that was never typed into it.

## Out of scope

- Anything that needs the machine already: reading Credential Manager as the
  signed-in user, editing a `.shim` a person can already write to, or a folder
  dropped into `PATH`.
- Detections of the runner by SmartScreen or Defender. An unsigned small
  executable copied under many names looks exactly like that, and it is written
  down in the README rather than fixed.
- A `.exe` that is not a shim of this project, whatever it is named.

---

# Политика безопасности

shimbox поддерживает один человек. SLA здесь нет и охоты за багами тоже: есть
две строки ниже о том, как добраться до кода и что будет дальше.

## Как сообщить об уязвимости

Сообщите приватно, через
[GitHub Security Advisories](https://github.com/goloveshko/shimbox/security/advisories/new),
а не в публичный issue, чтобы никому не пришлось запускать непочиненную сборку,
пока сообщение открыто. Язык — русский или английский, как удобнее.

Работает сообщение, которое можно воспроизвести. Версия и патч желательны, но не
обязательны.

Ответ приходит примерно в течение недели. Исправление выходит в следующем релизе,
advisory публикуется сразу после него — чтобы у тех, кто сидит на старой сборке,
был повод перейти.

## Что держит код

- **Секретов в конфиге нет.** В `.shim` лежит `envref` с именем записи в Windows
  Credential Manager, никогда не значение. Значение уходит только в блок
  окружения того процесса, который шим запускает.
- **Пара шима пишется один раз и молча не правится.** Существующий `.exe`
  перезаписывается, только если его ресурс версии называет этот проектом продукт,
  а одной из двух сборок раннера — исходный файл (docs/techspec.md 5.7): нужно оба
  имени, потому что первое есть и у менеджера; генерация поверх имени, которое уже
  есть в папке назначения, отклоняется.
- **Раннер читает один конфиг** — тот же имя, что у него сам, рядом с собой, и
  ничего больше.

Стоит сообщать о способе заставить раннер запустить не то, что ему не указывали,
о чтении секрета не тем процессом, которому он предназначен, и о пути, который
менеджер показывает или открывает, хотя его не вводили в форму.

## Вне рамок

- Всё, что требует уже полученного доступа к машине: чтение Credential Manager
  от имени вошедшего пользователя, правка `.shim`, которую можно сделать и так,
  папка, добавленная в `PATH`.
- Срабатывания SmartScreen или Defender на раннер. Небольшой неподписанный
  исполняемый файл, растиражированный под множеством имён, выглядит именно так;
  это описано в README, а не исправляется.
- `.exe`, который не является шимом этого проекта, как бы он ни назывался.
