(ns my-project.core
  (:require [clojure.string :as str]))

(defn greet [name]
  (str "Hello, " name))