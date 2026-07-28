(ns my-project.core-test
  (:require [clojure.test :refer [deftest is]]
            [my-project.core :as core]))

(deftest test-greet
  (is (= "Hello, World" (core/greet "World"))))